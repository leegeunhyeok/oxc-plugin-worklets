mod autoworkletization;
mod closure;
mod gesture_handler_autoworkletization;
mod globals;
mod layout_animation_autoworkletization;
mod options;
mod types;
mod worklet_factory;
use std::collections::HashSet;

pub use options::PluginOptions;

use oxc::allocator::{Allocator, Box as OxcBox, CloneIn};
use oxc::ast::ast::*;
use oxc::ast::AstBuilder;
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::span::{SourceType, SPAN};

use crate::autoworkletization::{
    get_args_to_workletize, is_reanimated_function_hook, is_reanimated_object_hook,
};
use crate::closure::{get_closure, get_closure_arrow};
use crate::gesture_handler_autoworkletization::is_gesture_object_event_callback_method;
use crate::globals::build_globals;
use crate::layout_animation_autoworkletization::is_layout_animation_callback_method;
use crate::types::{
    CONTEXT_OBJECT_MARKER, PLUGIN_VERSION, WORKLET_CLASS_FACTORY_SUFFIX, WORKLET_CLASS_MARKER,
};
use crate::worklet_factory::{hash, make_worklet_name};

#[derive(Debug)]
pub struct WorkletsError(pub String);

impl std::fmt::Display for WorkletsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transform error: {}", self.0)
    }
}

impl std::error::Error for WorkletsError {}

pub struct WorkletsVisitor<'a> {
    allocator: &'a Allocator,
    options: PluginOptions,
    globals: HashSet<String>,
    filename: String,
    worklet_number: u32,
    pending_insertions: Vec<(usize, Statement<'a>)>,
}

impl<'a> WorkletsVisitor<'a> {
    pub fn new(allocator: &'a Allocator, options: PluginOptions) -> Self {
        let globals = if options.strict_global {
            HashSet::new()
        } else {
            build_globals(&options.globals)
        };
        let filename = options
            .filename
            .as_deref()
            .unwrap_or("/dev/null")
            .to_string();
        Self {
            allocator,
            options,
            globals,
            filename,
            worklet_number: 1,
            pending_insertions: Vec::new(),
        }
    }

    pub fn visit_program(&mut self, program: &mut Program<'a>) -> Result<(), WorkletsError> {
        // Check for file-level 'worklet' directive
        let has_file_worklet = program
            .directives
            .iter()
            .any(|d| d.expression.value == "worklet");

        if has_file_worklet {
            program
                .directives
                .retain(|d| d.expression.value != "worklet");
            add_worklet_directives_to_top_level(program, self.allocator);
        }

        // Pre-pass: collect names that are passed as identifier arguments to auto-workletize hooks,
        // then add 'worklet' directives to the referenced definitions.
        let names_to_workletize = collect_referenced_worklet_names(&program.body);
        if !names_to_workletize.is_empty() {
            add_worklet_directives_to_referenced(program, self.allocator, &names_to_workletize);
        }

        // Process context objects (ObjectExpressions with __workletContextObject marker)
        process_context_objects(program, self.allocator);

        let mut i = 0;
        while i < program.body.len() {
            process_statement(&mut program.body[i], i, self)?;
            i += 1;
        }

        // Insert pending init_data declarations (sorted descending to maintain indices)
        if !self.pending_insertions.is_empty() {
            self.pending_insertions.sort_by(|a, b| b.0.cmp(&a.0));
            let insertions = std::mem::take(&mut self.pending_insertions);
            for (idx, stmt) in insertions {
                program.body.insert(idx, stmt);
            }
        }

        Ok(())
    }
}

// --- Statement processing ---

fn process_statement<'a>(
    stmt: &mut Statement<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<(), WorkletsError> {
    match stmt {
        Statement::FunctionDeclaration(func) => {
            if has_worklet_directive(func.body.as_ref()) {
                process_inner_worklets_in_function(func, stmt_idx, ctx)?;
                let replacement = transform_worklet_function(func, stmt_idx, ctx)?;
                *stmt = replacement;
            }
        }
        Statement::VariableDeclaration(var_decl) => {
            for declarator in var_decl.declarations.iter_mut() {
                if let Some(init) = &mut declarator.init {
                    process_expression(init, stmt_idx, ctx)?;
                }
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            process_expression(&mut expr_stmt.expression, stmt_idx, ctx)?;
        }
        Statement::ClassDeclaration(class) => {
            if !ctx.options.disable_worklet_classes && is_worklet_class(class) {
                let replacement = transform_worklet_class(class, stmt_idx, ctx)?;
                let ast = AstBuilder::new(ctx.allocator);
                let mut stmts = ast.vec_from_iter(replacement);
                // Replace the current statement with the first one,
                // we'll need to handle the second one differently
                if stmts.len() == 2 {
                    let second = stmts.pop().unwrap();
                    *stmt = stmts.pop().unwrap();
                    // We need to insert the second statement after the current one.
                    // Use a special approach: store it as a pending insertion at stmt_idx + 1
                    ctx.pending_insertions.push((stmt_idx + 1, second));
                } else if stmts.len() == 1 {
                    *stmt = stmts.pop().unwrap();
                }
            }
        }
        Statement::ExportDefaultDeclaration(export) => {
            if let ExportDefaultDeclarationKind::FunctionDeclaration(func) = &mut export.declaration
            {
                if has_worklet_directive(func.body.as_ref()) {
                    process_inner_worklets_in_function(func, stmt_idx, ctx)?;
                    let func_name = func.id.as_ref().map(|id| id.name.as_str());
                    let factory_call = build_factory_call(func, func_name, stmt_idx, ctx)?;
                    let ast = AstBuilder::new(ctx.allocator);
                    let call_expr = Expression::CallExpression(ast.alloc(factory_call));
                    export.declaration = ExportDefaultDeclarationKind::from(call_expr);
                }
            }
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &mut export.declaration {
                match decl {
                    Declaration::FunctionDeclaration(func) => {
                        if has_worklet_directive(func.body.as_ref()) {
                            process_inner_worklets_in_function(func, stmt_idx, ctx)?;
                            let func_name = func.id.as_ref().map(|id| id.name.as_str());
                            let name: &'a str = func
                                .id
                                .as_ref()
                                .map(|id| id.name.as_str())
                                .unwrap_or("_unnamed");
                            let factory_call = build_factory_call(func, func_name, stmt_idx, ctx)?;
                            let ast = AstBuilder::new(ctx.allocator);
                            let call_expr = Expression::CallExpression(ast.alloc(factory_call));
                            let var_decl = build_const_declaration(&ast, name, call_expr);
                            *decl = Declaration::VariableDeclaration(ast.alloc(var_decl));
                        }
                    }
                    Declaration::VariableDeclaration(var_decl) => {
                        for declarator in var_decl.declarations.iter_mut() {
                            if let Some(init) = &mut declarator.init {
                                process_expression(init, stmt_idx, ctx)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn process_expression<'a>(
    expr: &mut Expression<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<(), WorkletsError> {
    match expr {
        Expression::ArrowFunctionExpression(arrow) => {
            if has_worklet_directive_fn_body(&arrow.body) {
                process_inner_worklets_in_arrow(arrow, stmt_idx, ctx)?;
                let replacement = transform_worklet_arrow(arrow, stmt_idx, ctx)?;
                *expr = replacement;
                return Ok(());
            }
            process_inner_worklets_in_arrow(arrow, stmt_idx, ctx)?;
        }
        Expression::FunctionExpression(func) => {
            if has_worklet_directive(func.body.as_ref()) {
                process_inner_worklets_in_function(func, stmt_idx, ctx)?;
                let func_name = func.id.as_ref().map(|id| id.name.as_str());
                let factory_call = build_factory_call(func, func_name, stmt_idx, ctx)?;
                let ast = AstBuilder::new(ctx.allocator);
                *expr = Expression::CallExpression(ast.alloc(factory_call));
                return Ok(());
            }
            process_inner_worklets_in_function(func, stmt_idx, ctx)?;
        }
        Expression::CallExpression(call) => {
            process_call_expression(call, stmt_idx, ctx)?;
        }
        Expression::AssignmentExpression(assign) => {
            process_expression(&mut assign.right, stmt_idx, ctx)?;
        }
        Expression::SequenceExpression(seq) => {
            for inner in seq.expressions.iter_mut() {
                process_expression(inner, stmt_idx, ctx)?;
            }
        }
        Expression::ConditionalExpression(cond) => {
            process_expression(&mut cond.consequent, stmt_idx, ctx)?;
            process_expression(&mut cond.alternate, stmt_idx, ctx)?;
        }
        Expression::LogicalExpression(logical) => {
            process_expression(&mut logical.right, stmt_idx, ctx)?;
        }
        Expression::ObjectExpression(obj) => {
            for prop in obj.properties.iter_mut() {
                if let ObjectPropertyKind::ObjectProperty(p) = prop {
                    process_expression(&mut p.value, stmt_idx, ctx)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn process_call_expression<'a>(
    call: &mut CallExpression<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<(), WorkletsError> {
    let callee_name = get_callee_name(&call.callee).map(|s| s.to_string());
    let mut handled = false;

    if let Some(ref name) = callee_name {
        let is_func = is_reanimated_function_hook(name);
        let is_obj = is_reanimated_object_hook(name);

        if is_func || is_obj {
            if let Some(arg_indices) = get_args_to_workletize(name) {
                let indices: Vec<usize> = arg_indices.to_vec();
                for idx in indices {
                    if idx < call.arguments.len() {
                        process_autoworkletize_arg(
                            &mut call.arguments[idx],
                            stmt_idx,
                            ctx,
                            is_func,
                            is_obj,
                        )?;
                    }
                }
                handled = true;
            }
        }
    }

    if !handled && is_gesture_object_event_callback_method(&call.callee) {
        for arg in call.arguments.iter_mut() {
            process_autoworkletize_arg(arg, stmt_idx, ctx, true, true)?;
        }
    }

    if is_layout_animation_callback_method(&call.callee) {
        for arg in call.arguments.iter_mut() {
            process_autoworkletize_arg(arg, stmt_idx, ctx, true, false)?;
        }
    }

    // Recurse into callee for chained calls
    if let Expression::CallExpression(callee_call) = &mut call.callee {
        process_call_expression(callee_call, stmt_idx, ctx)?;
    }

    // Recurse into arguments for nested calls
    for arg in call.arguments.iter_mut() {
        if let Argument::CallExpression(inner) = arg {
            process_call_expression(inner, stmt_idx, ctx)?;
        }
    }

    Ok(())
}

fn process_autoworkletize_arg<'a>(
    arg: &mut Argument<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
    accept_function: bool,
    accept_object: bool,
) -> Result<(), WorkletsError> {
    match arg {
        Argument::ArrowFunctionExpression(arrow) if accept_function => {
            process_inner_worklets_in_arrow(arrow, stmt_idx, ctx)?;
            let replacement = transform_worklet_arrow(arrow, stmt_idx, ctx)?;
            *arg = Argument::from(replacement);
        }
        Argument::FunctionExpression(func) if accept_function => {
            process_inner_worklets_in_function(func, stmt_idx, ctx)?;
            let func_name = func.id.as_ref().map(|id| id.name.as_str());
            let factory_call = build_factory_call(func, func_name, stmt_idx, ctx)?;
            let ast = AstBuilder::new(ctx.allocator);
            *arg = Argument::from(Expression::CallExpression(ast.alloc(factory_call)));
        }
        Argument::ObjectExpression(obj) if accept_object => {
            process_workletizable_object(obj, stmt_idx, ctx, accept_function)?;
        }
        _ => {}
    }
    Ok(())
}

fn get_callee_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        Expression::SequenceExpression(seq) => seq.expressions.last().and_then(get_callee_name),
        _ => None,
    }
}

fn has_worklet_directive(body: Option<&OxcBox<'_, FunctionBody<'_>>>) -> bool {
    body.is_some_and(|b| has_worklet_directive_fn_body(b))
}

fn has_worklet_directive_fn_body(body: &FunctionBody) -> bool {
    body.directives
        .iter()
        .any(|d| d.expression.value == "worklet")
}

// --- Inner worklet processing ---

fn process_inner_worklets_in_function<'a>(
    func: &mut Function<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<(), WorkletsError> {
    if let Some(body) = &mut func.body {
        for stmt in body.statements.iter_mut() {
            process_inner_stmt(stmt, stmt_idx, ctx)?;
        }
    }
    Ok(())
}

fn process_inner_worklets_in_arrow<'a>(
    arrow: &mut ArrowFunctionExpression<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<(), WorkletsError> {
    for stmt in arrow.body.statements.iter_mut() {
        process_inner_stmt(stmt, stmt_idx, ctx)?;
    }
    Ok(())
}

fn process_inner_stmt<'a>(
    stmt: &mut Statement<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<(), WorkletsError> {
    match stmt {
        Statement::VariableDeclaration(v) => {
            for d in v.declarations.iter_mut() {
                if let Some(init) = &mut d.init {
                    process_expression(init, stmt_idx, ctx)?;
                }
            }
        }
        Statement::ExpressionStatement(es) => {
            process_expression(&mut es.expression, stmt_idx, ctx)?;
        }
        Statement::FunctionDeclaration(func) => {
            if has_worklet_directive(func.body.as_ref()) {
                process_inner_worklets_in_function(func, stmt_idx, ctx)?;
                let func_name = func.id.as_ref().map(|id| id.name.as_str());
                let factory_call = build_factory_call(func, func_name, stmt_idx, ctx)?;
                let name: &'a str = func
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("_unnamed");
                let ast = AstBuilder::new(ctx.allocator);
                let vd = build_const_declaration(
                    &ast,
                    name,
                    Expression::CallExpression(ast.alloc(factory_call)),
                );
                *stmt = Statement::VariableDeclaration(ast.alloc(vd));
            } else {
                process_inner_worklets_in_function(func, stmt_idx, ctx)?;
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &mut ret.argument {
                process_expression(arg, stmt_idx, ctx)?;
            }
        }
        Statement::IfStatement(ifs) => {
            process_inner_stmt(&mut ifs.consequent, stmt_idx, ctx)?;
            if let Some(alt) = &mut ifs.alternate {
                process_inner_stmt(alt, stmt_idx, ctx)?;
            }
        }
        Statement::BlockStatement(block) => {
            for s in block.body.iter_mut() {
                process_inner_stmt(s, stmt_idx, ctx)?;
            }
        }
        _ => {}
    }
    Ok(())
}

// --- Worklet transformation ---

fn transform_worklet_function<'a>(
    func: &mut Function<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<Statement<'a>, WorkletsError> {
    let func_name = func.id.as_ref().map(|id| id.name.as_str());
    let factory_call = build_factory_call(func, func_name, stmt_idx, ctx)?;
    let name: &'a str = func
        .id
        .as_ref()
        .map(|id| id.name.as_str())
        .unwrap_or("_unnamed");
    let ast = AstBuilder::new(ctx.allocator);
    let vd = build_const_declaration(
        &ast,
        name,
        Expression::CallExpression(ast.alloc(factory_call)),
    );
    Ok(Statement::VariableDeclaration(ast.alloc(vd)))
}

fn transform_worklet_arrow<'a>(
    arrow: &mut ArrowFunctionExpression<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<Expression<'a>, WorkletsError> {
    arrow
        .body
        .directives
        .retain(|d| d.expression.value != "worklet");

    let closure_vars = get_closure_arrow(arrow, &ctx.globals, ctx.options.strict_global);

    let wn = ctx.worklet_number;
    ctx.worklet_number += 1;
    let (worklet_name, react_name) = make_worklet_name(None, &ctx.filename, wn);

    let code_string =
        generate_worklet_code_string_from_arrow(ctx.allocator, arrow, &worklet_name, &closure_vars);
    let worklet_hash = hash(&code_string);

    let ast = AstBuilder::new(ctx.allocator);
    let params = arrow.params.clone_in(ctx.allocator);
    let body = arrow.body.clone_in(ctx.allocator);

    let func_expr = ast.alloc(ast.function(
        SPAN,
        FunctionType::FunctionExpression,
        None::<BindingIdentifier>,
        false,
        arrow.r#async,
        false,
        None::<TSTypeParameterDeclaration>,
        None::<TSThisParameter>,
        params,
        None::<TSTypeAnnotation>,
        Some(body),
    ));

    // Arena-allocate the strings
    let react_name = ctx.allocator.alloc_str(&react_name);
    let worklet_name = ctx.allocator.alloc_str(&worklet_name);
    let code_string = ctx.allocator.alloc_str(&code_string);

    let factory_call = build_factory_call_inner(
        ctx,
        func_expr,
        react_name,
        worklet_name,
        code_string,
        worklet_hash,
        &closure_vars,
        stmt_idx,
    )?;

    let ast = AstBuilder::new(ctx.allocator);
    Ok(Expression::CallExpression(ast.alloc(factory_call)))
}

fn build_factory_call<'a>(
    func: &mut Function<'a>,
    func_name: Option<&str>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<CallExpression<'a>, WorkletsError> {
    if let Some(body) = &mut func.body {
        body.directives.retain(|d| d.expression.value != "worklet");
    }

    let closure_vars = get_closure(func, &ctx.globals, ctx.options.strict_global);

    let wn = ctx.worklet_number;
    ctx.worklet_number += 1;
    let (worklet_name, react_name) = make_worklet_name(func_name, &ctx.filename, wn);

    let code_string = generate_worklet_code_string_from_function(
        ctx.allocator,
        func,
        &worklet_name,
        &closure_vars,
        func_name,
    );
    let worklet_hash = hash(&code_string);

    let ast = AstBuilder::new(ctx.allocator);
    let params = func.params.clone_in(ctx.allocator);
    let body = func.body.clone_in(ctx.allocator);

    let func_expr = ast.alloc(ast.function(
        SPAN,
        FunctionType::FunctionExpression,
        None::<BindingIdentifier>,
        func.generator,
        func.r#async,
        false,
        None::<TSTypeParameterDeclaration>,
        None::<TSThisParameter>,
        params,
        None::<TSTypeAnnotation>,
        body,
    ));

    // Arena-allocate the strings
    let react_name = ctx.allocator.alloc_str(&react_name);
    let worklet_name = ctx.allocator.alloc_str(&worklet_name);
    let code_string = ctx.allocator.alloc_str(&code_string);

    build_factory_call_inner(
        ctx,
        func_expr,
        react_name,
        worklet_name,
        code_string,
        worklet_hash,
        &closure_vars,
        stmt_idx,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_factory_call_inner<'a>(
    ctx: &mut WorkletsVisitor<'a>,
    func_expr: OxcBox<'a, Function<'a>>,
    react_name: &'a str,
    worklet_name: &'a str,
    code_string: &'a str,
    worklet_hash: u64,
    closure_vars: &[String],
    stmt_idx: usize,
) -> Result<CallExpression<'a>, WorkletsError> {
    let ast = AstBuilder::new(ctx.allocator);
    let is_release = ctx.options.is_release;
    let should_include_init_data = !ctx.options.omit_native_only_data;
    let init_data_name_string = format!("_worklet_{}_init_data", worklet_hash);
    let init_data_name: &'a str = ctx.allocator.alloc_str(&init_data_name_string);

    if should_include_init_data {
        let init_stmt = build_init_data_declaration(
            &ast,
            init_data_name,
            code_string,
            &ctx.filename,
            is_release,
            &ctx.options,
            ctx.allocator,
        );
        ctx.pending_insertions.push((stmt_idx, init_stmt));
    }

    let mut stmts = ast.vec();

    if !is_release {
        let line_offset = if closure_vars.is_empty() {
            1.0
        } else {
            1.0 - (closure_vars.len() as f64) - 2.0
        };
        stmts.push(build_error_stmt(&ast, line_offset));
    }

    // const reactName = <func>
    stmts.push(Statement::from(Declaration::VariableDeclaration(
        ast.alloc(ast.variable_declaration(
            SPAN,
            VariableDeclarationKind::Const,
            ast.vec1(ast.variable_declarator(
                SPAN,
                VariableDeclarationKind::Const,
                ast.binding_pattern_binding_identifier(SPAN, react_name),
                None::<TSTypeAnnotation>,
                Some(Expression::FunctionExpression(func_expr)),
                false,
            )),
            false,
        )),
    )));

    // reactName.__closure = { ... }
    stmts.push(build_closure_assignment(
        &ast,
        react_name,
        closure_vars,
        ctx.allocator,
    ));

    // reactName.__workletHash = hash
    stmts.push(build_member_assign_number(
        &ast,
        react_name,
        "__workletHash",
        worklet_hash as f64,
    ));

    if !is_release {
        stmts.push(build_member_assign_string(
            &ast,
            react_name,
            "__pluginVersion",
            PLUGIN_VERSION,
        ));
    }

    if should_include_init_data {
        stmts.push(build_member_assign_ident(
            &ast,
            react_name,
            "__initData",
            init_data_name,
        ));
    }

    if !is_release {
        stmts.push(build_member_assign_ident(
            &ast,
            react_name,
            "__stackDetails",
            "_e",
        ));
    }

    stmts.push(ast.statement_return(SPAN, Some(ast.expression_identifier(SPAN, react_name))));

    // Build factory params: ({ initDataName, ...closureVars })
    let mut param_props = ast.vec();
    if should_include_init_data {
        param_props.push(ast.binding_property(
            SPAN,
            ast.property_key_static_identifier(SPAN, init_data_name),
            ast.binding_pattern_binding_identifier(SPAN, init_data_name),
            true,
            false,
        ));
    }
    for var in closure_vars {
        let var_str: &'a str = ctx.allocator.alloc_str(var.as_str());
        param_props.push(ast.binding_property(
            SPAN,
            ast.property_key_static_identifier(SPAN, var_str),
            ast.binding_pattern_binding_identifier(SPAN, var_str),
            true,
            false,
        ));
    }

    let obj_pattern =
        ast.binding_pattern_object_pattern(SPAN, param_props, None::<BindingRestElement>);
    let factory_params = ast.formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec1(ast.formal_parameter(
            SPAN,
            ast.vec(),
            obj_pattern,
            None::<TSTypeAnnotation>,
            None::<Expression>,
            false,
            None,
            false,
            false,
        )),
        None::<FormalParameterRest>,
    );

    let factory_body = ast.function_body(SPAN, ast.vec(), stmts);
    let factory_name_string = format!("{}Factory", worklet_name);
    let factory_name: &'a str = ctx.allocator.alloc_str(&factory_name_string);

    let factory = ast.function(
        SPAN,
        FunctionType::FunctionExpression,
        Some(ast.binding_identifier(SPAN, factory_name)),
        false,
        false,
        false,
        None::<TSTypeParameterDeclaration>,
        None::<TSThisParameter>,
        factory_params,
        None::<TSTypeAnnotation>,
        Some(factory_body),
    );

    // Build call arg: ({ initDataName, ...closureVars })
    let mut call_props = ast.vec();
    if should_include_init_data {
        call_props.push(ast.object_property_kind_object_property(
            SPAN,
            PropertyKind::Init,
            ast.property_key_static_identifier(SPAN, init_data_name),
            ast.expression_identifier(SPAN, init_data_name),
            false,
            true,
            false,
        ));
    }
    for var in closure_vars {
        let var_str: &'a str = ctx.allocator.alloc_str(var.as_str());
        call_props.push(ast.object_property_kind_object_property(
            SPAN,
            PropertyKind::Init,
            ast.property_key_static_identifier(SPAN, var_str),
            ast.expression_identifier(SPAN, var_str),
            false,
            true,
            false,
        ));
    }
    let call_arg = ast.expression_object(SPAN, call_props);

    Ok(ast.call_expression(
        SPAN,
        Expression::FunctionExpression(ast.alloc(factory)),
        None::<TSTypeParameterInstantiation>,
        ast.vec1(Argument::from(call_arg)),
        false,
    ))
}

// --- Code generation ---

fn generate_worklet_code_string_from_function(
    _allocator: &Allocator,
    func: &Function<'_>,
    worklet_name: &str,
    closure_vars: &[String],
    original_name: Option<&str>,
) -> String {
    let temp_alloc = Allocator::default();
    let ast = AstBuilder::new(&temp_alloc);

    let params = func.params.clone_in(&temp_alloc);
    let body = func.body.clone_in(&temp_alloc);

    let wf = ast.function(
        SPAN,
        FunctionType::FunctionExpression,
        Some(ast.binding_identifier(SPAN, temp_alloc.alloc_str(worklet_name))),
        func.generator,
        func.r#async,
        false,
        None::<TSTypeParameterDeclaration>,
        None::<TSThisParameter>,
        params,
        None::<TSTypeAnnotation>,
        body,
    );

    let expr = Expression::FunctionExpression(ast.alloc(wf));
    let stmt = ast.statement_expression(SPAN, expr);
    let program = ast.program(
        SPAN,
        SourceType::mjs(),
        "",
        ast.vec(),
        None,
        ast.vec(),
        ast.vec1(stmt),
    );

    let code = Codegen::new()
        .with_options(CodegenOptions::minify())
        .build(&program)
        .code;
    let code = code.trim_end_matches(';');

    inject_closure_and_recursion(code, closure_vars, original_name)
}

fn generate_worklet_code_string_from_arrow(
    _allocator: &Allocator,
    arrow: &ArrowFunctionExpression<'_>,
    worklet_name: &str,
    closure_vars: &[String],
) -> String {
    let temp_alloc = Allocator::default();
    let ast = AstBuilder::new(&temp_alloc);

    let params = arrow.params.clone_in(&temp_alloc);
    let body = arrow.body.clone_in(&temp_alloc);

    let wf = ast.function(
        SPAN,
        FunctionType::FunctionExpression,
        Some(ast.binding_identifier(SPAN, temp_alloc.alloc_str(worklet_name))),
        false,
        arrow.r#async,
        false,
        None::<TSTypeParameterDeclaration>,
        None::<TSThisParameter>,
        params,
        None::<TSTypeAnnotation>,
        Some(body),
    );

    let expr = Expression::FunctionExpression(ast.alloc(wf));
    let stmt = ast.statement_expression(SPAN, expr);
    let program = ast.program(
        SPAN,
        SourceType::mjs(),
        "",
        ast.vec(),
        None,
        ast.vec(),
        ast.vec1(stmt),
    );

    let code = Codegen::new()
        .with_options(CodegenOptions::minify())
        .build(&program)
        .code;
    let code = code.trim_end_matches(';');

    inject_closure_and_recursion(code, closure_vars, None)
}

fn inject_closure_and_recursion(
    code: &str,
    closure_vars: &[String],
    original_name: Option<&str>,
) -> String {
    if closure_vars.is_empty() && original_name.is_none() {
        return code.to_string();
    }

    if let Some(brace_pos) = code.find('{') {
        let mut result = String::with_capacity(code.len() + 100);
        result.push_str(&code[..=brace_pos]);

        if !closure_vars.is_empty() {
            result.push_str("const{");
            for (i, var) in closure_vars.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                result.push_str(var);
            }
            result.push_str("}=this.__closure;");
        }

        if let Some(name) = original_name {
            let body_part = &code[brace_pos + 1..];
            if body_part.contains(name) {
                result.push_str("const ");
                result.push_str(name);
                result.push_str("=this._recur;");
            }
        }

        result.push_str(&code[brace_pos + 1..]);
        result
    } else {
        code.to_string()
    }
}

// --- AST helper builders ---

fn build_const_declaration<'a>(
    ast: &AstBuilder<'a>,
    name: &'a str,
    init: Expression<'a>,
) -> VariableDeclaration<'a> {
    ast.variable_declaration(
        SPAN,
        VariableDeclarationKind::Const,
        ast.vec1(ast.variable_declarator(
            SPAN,
            VariableDeclarationKind::Const,
            ast.binding_pattern_binding_identifier(SPAN, name),
            None::<TSTypeAnnotation>,
            Some(init),
            false,
        )),
        false,
    )
}

fn build_init_data_declaration<'a>(
    ast: &AstBuilder<'a>,
    init_data_name: &'a str,
    code_string: &'a str,
    filename: &str,
    is_release: bool,
    options: &PluginOptions,
    allocator: &'a Allocator,
) -> Statement<'a> {
    let mut props = ast.vec();
    props.push(ast.object_property_kind_object_property(
        SPAN,
        PropertyKind::Init,
        ast.property_key_static_identifier(SPAN, "code"),
        ast.expression_string_literal(SPAN, code_string, None),
        false,
        false,
        false,
    ));

    if !is_release {
        let location = if options.relative_source_location {
            if let Some(cwd) = &options.cwd {
                std::path::Path::new(filename)
                    .strip_prefix(cwd)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| filename.to_string())
            } else {
                filename.to_string()
            }
        } else {
            filename.to_string()
        };
        let location: &'a str = allocator.alloc_str(&location);
        props.push(ast.object_property_kind_object_property(
            SPAN,
            PropertyKind::Init,
            ast.property_key_static_identifier(SPAN, "location"),
            ast.expression_string_literal(SPAN, location, None),
            false,
            false,
            false,
        ));
    }

    let obj = ast.expression_object(SPAN, props);
    let decl = ast.variable_declaration(
        SPAN,
        VariableDeclarationKind::Const,
        ast.vec1(ast.variable_declarator(
            SPAN,
            VariableDeclarationKind::Const,
            ast.binding_pattern_binding_identifier(SPAN, init_data_name),
            None::<TSTypeAnnotation>,
            Some(obj),
            false,
        )),
        false,
    );
    Statement::from(Declaration::VariableDeclaration(ast.alloc(decl)))
}

fn build_error_stmt<'a>(ast: &AstBuilder<'a>, line_offset: f64) -> Statement<'a> {
    let global_error = Expression::StaticMemberExpression(ast.alloc(ast.static_member_expression(
        SPAN,
        ast.expression_identifier(SPAN, "global"),
        ast.identifier_name(SPAN, "Error"),
        false,
    )));

    let none_type_params: Option<OxcBox<'_, TSTypeParameterInstantiation<'_>>> = None;
    let array = ast.expression_array(
        SPAN,
        ast.vec_from_iter([
            ArrayExpressionElement::from(ast.expression_new(
                SPAN,
                global_error,
                none_type_params,
                ast.vec(),
            )),
            ArrayExpressionElement::from(ast.expression_numeric_literal(
                SPAN,
                line_offset,
                None,
                NumberBase::Decimal,
            )),
            ArrayExpressionElement::from(ast.expression_unary(
                SPAN,
                UnaryOperator::UnaryNegation,
                ast.expression_numeric_literal(SPAN, 27.0, None, NumberBase::Decimal),
            )),
        ]),
    );

    Statement::from(Declaration::VariableDeclaration(ast.alloc(
        ast.variable_declaration(
            SPAN,
            VariableDeclarationKind::Const,
            ast.vec1(ast.variable_declarator(
                SPAN,
                VariableDeclarationKind::Const,
                ast.binding_pattern_binding_identifier(SPAN, "_e"),
                None::<TSTypeAnnotation>,
                Some(array),
                false,
            )),
            false,
        ),
    )))
}

fn build_closure_assignment<'a>(
    ast: &AstBuilder<'a>,
    react_name: &'a str,
    closure_vars: &[String],
    allocator: &'a Allocator,
) -> Statement<'a> {
    let props = ast.vec_from_iter(closure_vars.iter().map(|var| {
        let var_str: &'a str = allocator.alloc_str(var.as_str());
        ast.object_property_kind_object_property(
            SPAN,
            PropertyKind::Init,
            ast.property_key_static_identifier(SPAN, var_str),
            ast.expression_identifier(SPAN, var_str),
            false,
            true,
            false,
        )
    }));
    let obj = ast.expression_object(SPAN, props);
    build_member_assign(ast, react_name, "__closure", obj)
}

fn build_member_assign<'a>(
    ast: &AstBuilder<'a>,
    obj_name: &'a str,
    prop_name: &'a str,
    value: Expression<'a>,
) -> Statement<'a> {
    let target = AssignmentTarget::StaticMemberExpression(ast.alloc(ast.static_member_expression(
        SPAN,
        ast.expression_identifier(SPAN, obj_name),
        ast.identifier_name(SPAN, prop_name),
        false,
    )));
    ast.statement_expression(
        SPAN,
        ast.expression_assignment(SPAN, AssignmentOperator::Assign, target, value),
    )
}

fn build_member_assign_number<'a>(
    ast: &AstBuilder<'a>,
    obj_name: &'a str,
    prop_name: &'a str,
    value: f64,
) -> Statement<'a> {
    build_member_assign(
        ast,
        obj_name,
        prop_name,
        ast.expression_numeric_literal(SPAN, value, None, NumberBase::Decimal),
    )
}

fn build_member_assign_string<'a>(
    ast: &AstBuilder<'a>,
    obj_name: &'a str,
    prop_name: &'a str,
    value: &'a str,
) -> Statement<'a> {
    build_member_assign(
        ast,
        obj_name,
        prop_name,
        ast.expression_string_literal(SPAN, value, None),
    )
}

fn build_member_assign_ident<'a>(
    ast: &AstBuilder<'a>,
    obj_name: &'a str,
    prop_name: &'a str,
    value_name: &'a str,
) -> Statement<'a> {
    build_member_assign(
        ast,
        obj_name,
        prop_name,
        ast.expression_identifier(SPAN, value_name),
    )
}

/// Process workletizable object: workletize each property value that is a function.
fn process_workletizable_object<'a>(
    obj: &mut ObjectExpression<'a>,
    stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
    accept_function: bool,
) -> Result<(), WorkletsError> {
    for prop in obj.properties.iter_mut() {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            if accept_function {
                match &mut p.value {
                    Expression::ArrowFunctionExpression(arrow) => {
                        process_inner_worklets_in_arrow(arrow, stmt_idx, ctx)?;
                        let r = transform_worklet_arrow(arrow, stmt_idx, ctx)?;
                        p.value = r;
                    }
                    Expression::FunctionExpression(func) => {
                        process_inner_worklets_in_function(func, stmt_idx, ctx)?;
                        let n = func.id.as_ref().map(|id| id.name.as_str());
                        let fc = build_factory_call(func, n, stmt_idx, ctx)?;
                        let ast = AstBuilder::new(ctx.allocator);
                        p.value = Expression::CallExpression(ast.alloc(fc));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

// --- Context Object processing ---

/// Check if an ObjectExpression has the __workletContextObject marker property.
fn is_context_object(obj: &ObjectExpression) -> bool {
    obj.properties.iter().any(|prop| {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            if let PropertyKey::StaticIdentifier(id) = &p.key {
                return id.name == CONTEXT_OBJECT_MARKER;
            }
        }
        false
    })
}

/// Process all context objects in the program.
/// For each ObjectExpression with __workletContextObject:
/// 1. Remove the marker property
/// 2. Add a __workletContextObjectFactory property (a function returning the object with 'worklet' directive)
fn process_context_objects<'a>(program: &mut Program<'a>, allocator: &'a Allocator) {
    for stmt in program.body.iter_mut() {
        match stmt {
            Statement::VariableDeclaration(var_decl) => {
                for d in var_decl.declarations.iter_mut() {
                    if let Some(Expression::ObjectExpression(obj)) = &mut d.init {
                        if is_context_object(obj) {
                            process_context_object(obj, allocator);
                        }
                    }
                }
            }
            Statement::ExpressionStatement(es) => {
                if let Expression::AssignmentExpression(assign) = &mut es.expression {
                    if let Expression::ObjectExpression(obj) = &mut assign.right {
                        if is_context_object(obj) {
                            process_context_object(obj, allocator);
                        }
                    }
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::VariableDeclaration(var_decl)) = &mut export.declaration {
                    for d in var_decl.declarations.iter_mut() {
                        if let Some(Expression::ObjectExpression(obj)) = &mut d.init {
                            if is_context_object(obj) {
                                process_context_object(obj, allocator);
                            }
                        }
                    }
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ObjectExpression(obj) = &mut export.declaration
                {
                    if is_context_object(obj) {
                        process_context_object(obj, allocator);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Transform a context object: remove marker and add factory function property.
fn process_context_object<'a>(obj: &mut ObjectExpression<'a>, allocator: &'a Allocator) {
    let ast = AstBuilder::new(allocator);

    // Remove the __workletContextObject marker property
    obj.properties.retain(|prop| {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            if let PropertyKey::StaticIdentifier(id) = &p.key {
                return id.name != CONTEXT_OBJECT_MARKER;
            }
        }
        true
    });

    // Clone the object to create the return value of the factory
    let cloned_obj = obj.clone_in(allocator);
    let return_stmt = ast.statement_return(
        SPAN,
        Some(Expression::ObjectExpression(ast.alloc(cloned_obj))),
    );

    let factory_body = ast.function_body(
        SPAN,
        ast.vec1(ast.directive(SPAN, ast.string_literal(SPAN, "worklet", None), "worklet")),
        ast.vec1(return_stmt),
    );

    let factory_func = ast.function(
        SPAN,
        FunctionType::FunctionExpression,
        None::<BindingIdentifier>,
        false,
        false,
        false,
        None::<TSTypeParameterDeclaration>,
        None::<TSThisParameter>,
        ast.formal_parameters(
            SPAN,
            FormalParameterKind::FormalParameter,
            ast.vec(),
            None::<FormalParameterRest>,
        ),
        None::<TSTypeAnnotation>,
        Some(factory_body),
    );

    let factory_name = format!("{}Factory", CONTEXT_OBJECT_MARKER);
    let factory_name: &'a str = allocator.alloc_str(&factory_name);

    // Add __workletContextObjectFactory property to the object
    obj.properties
        .push(ast.object_property_kind_object_property(
            SPAN,
            PropertyKind::Init,
            ast.property_key_static_identifier(SPAN, factory_name),
            Expression::FunctionExpression(ast.alloc(factory_func)),
            false,
            false,
            false,
        ));
}

// --- Referenced Worklet Collection ---

/// Collect names of identifiers that are passed as arguments to auto-workletize hooks.
/// These need to have 'worklet' directives added to their definitions.
fn collect_referenced_worklet_names(stmts: &[Statement]) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in stmts {
        collect_worklet_names_from_stmt(stmt, &mut names);
    }
    names
}

fn collect_worklet_names_from_stmt(stmt: &Statement, names: &mut HashSet<String>) {
    match stmt {
        Statement::VariableDeclaration(var_decl) => {
            for d in &var_decl.declarations {
                if let Some(init) = &d.init {
                    collect_worklet_names_from_expr(init, names);
                }
            }
        }
        Statement::ExpressionStatement(es) => {
            collect_worklet_names_from_expr(&es.expression, names);
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                if let Declaration::VariableDeclaration(var_decl) = decl {
                    for d in &var_decl.declarations {
                        if let Some(init) = &d.init {
                            collect_worklet_names_from_expr(init, names);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_worklet_names_from_expr(expr: &Expression, names: &mut HashSet<String>) {
    match expr {
        Expression::CallExpression(call) => {
            let callee_name = get_callee_name(&call.callee).map(|s| s.to_string());
            if let Some(ref name) = callee_name {
                let is_func = is_reanimated_function_hook(name);
                let is_obj = is_reanimated_object_hook(name);
                if is_func || is_obj {
                    if let Some(arg_indices) = get_args_to_workletize(name) {
                        for &idx in arg_indices {
                            if idx < call.arguments.len() {
                                if let Argument::Identifier(ident) = &call.arguments[idx] {
                                    names.insert(ident.name.to_string());
                                }
                            }
                        }
                    }
                }
            }
            // Recurse into callee for chained calls
            if let Expression::CallExpression(inner) = &call.callee {
                collect_worklet_names_from_expr(
                    &Expression::CallExpression(inner.clone_in(&Allocator::default())),
                    names,
                );
            }
        }
        Expression::AssignmentExpression(assign) => {
            collect_worklet_names_from_expr(&assign.right, names);
        }
        _ => {}
    }
}

/// Add 'worklet' directives to the definitions of referenced worklet names.
fn add_worklet_directives_to_referenced<'a>(
    program: &mut Program<'a>,
    allocator: &'a Allocator,
    names: &HashSet<String>,
) {
    let ast = AstBuilder::new(allocator);
    for stmt in program.body.iter_mut() {
        match stmt {
            Statement::FunctionDeclaration(func) => {
                if let Some(id) = &func.id {
                    if names.contains(id.name.as_str()) {
                        add_worklet_directive_to_func_body(func, &ast);
                    }
                }
            }
            Statement::VariableDeclaration(var_decl) => {
                for d in var_decl.declarations.iter_mut() {
                    if let BindingPattern::BindingIdentifier(id) = &d.id {
                        if names.contains(id.name.as_str()) {
                            if let Some(init) = &mut d.init {
                                add_worklet_directive_to_expr(init, &ast, allocator);
                            }
                        }
                    }
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(decl) = &mut export.declaration {
                    match decl {
                        Declaration::FunctionDeclaration(func) => {
                            if let Some(id) = &func.id {
                                if names.contains(id.name.as_str()) {
                                    add_worklet_directive_to_func_body(func, &ast);
                                }
                            }
                        }
                        Declaration::VariableDeclaration(var_decl) => {
                            for d in var_decl.declarations.iter_mut() {
                                if let BindingPattern::BindingIdentifier(id) = &d.id {
                                    if names.contains(id.name.as_str()) {
                                        if let Some(init) = &mut d.init {
                                            add_worklet_directive_to_expr(init, &ast, allocator);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        // Also check assignment expressions (for reassigned variables)
        if let Statement::ExpressionStatement(es) = stmt {
            if let Expression::AssignmentExpression(assign) = &mut es.expression {
                if let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left {
                    if names.contains(id.name.as_str()) {
                        add_worklet_directive_to_expr(&mut assign.right, &ast, allocator);
                    }
                }
            }
        }
    }
}

fn add_worklet_directives_to_top_level<'a>(program: &mut Program<'a>, allocator: &'a Allocator) {
    let ast = AstBuilder::new(allocator);
    for stmt in program.body.iter_mut() {
        match stmt {
            Statement::FunctionDeclaration(func) => {
                add_worklet_directive_to_func_body(func, &ast);
            }
            Statement::VariableDeclaration(var_decl) => {
                for d in var_decl.declarations.iter_mut() {
                    if let Some(init) = &mut d.init {
                        add_worklet_directive_to_expr(init, &ast, allocator);
                    }
                }
            }
            Statement::ClassDeclaration(class) => {
                add_worklet_class_marker(class, &ast);
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(decl) = &mut export.declaration {
                    match decl {
                        Declaration::FunctionDeclaration(func) => {
                            add_worklet_directive_to_func_body(func, &ast);
                        }
                        Declaration::VariableDeclaration(var_decl) => {
                            for d in var_decl.declarations.iter_mut() {
                                if let Some(init) = &mut d.init {
                                    add_worklet_directive_to_expr(init, &ast, allocator);
                                }
                            }
                        }
                        Declaration::ClassDeclaration(class) => {
                            add_worklet_class_marker(class, &ast);
                        }
                        _ => {}
                    }
                }
            }
            Statement::ExportDefaultDeclaration(export) => match &mut export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    add_worklet_directive_to_func_body(func, &ast);
                }
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    add_worklet_class_marker(class, &ast);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn add_worklet_directive_to_func_body<'a>(func: &mut Function<'a>, ast: &AstBuilder<'a>) {
    if let Some(body) = &mut func.body {
        if !body
            .directives
            .iter()
            .any(|d| d.expression.value == "worklet")
        {
            body.directives.push(ast.directive(
                SPAN,
                ast.string_literal(SPAN, "worklet", None),
                "worklet",
            ));
        }
    }
}

fn add_worklet_directive_to_expr<'a>(
    expr: &mut Expression<'a>,
    ast: &AstBuilder<'a>,
    allocator: &'a Allocator,
) {
    match expr {
        Expression::ArrowFunctionExpression(arrow) => {
            if arrow.expression {
                if let Some(Statement::ExpressionStatement(es)) = arrow.body.statements.first() {
                    let ret_expr = es.expression.clone_in(allocator);
                    arrow.body.statements.clear();
                    arrow
                        .body
                        .statements
                        .push(ast.statement_return(SPAN, Some(ret_expr)));
                    arrow.expression = false;
                }
            }
            if !arrow
                .body
                .directives
                .iter()
                .any(|d| d.expression.value == "worklet")
            {
                arrow.body.directives.push(ast.directive(
                    SPAN,
                    ast.string_literal(SPAN, "worklet", None),
                    "worklet",
                ));
            }
        }
        Expression::FunctionExpression(func) => {
            add_worklet_directive_to_func_body(func, ast);
        }
        Expression::ObjectExpression(obj) => {
            // Check if any object method uses `this` — if so, it's an implicit context object
            if is_implicit_context_object(obj) {
                // Add __workletContextObject marker (will be processed later by process_context_objects)
                add_context_object_marker(obj, ast);
            } else {
                // Otherwise, process each property individually
                process_worklet_aggregator_object(obj, ast, allocator);
            }
        }
        _ => {}
    }
}

/// Check if an ObjectExpression has any ObjectMethod that uses `this`.
fn is_implicit_context_object(obj: &ObjectExpression) -> bool {
    use oxc::ast_visit::Visit;
    use oxc::syntax::scope::ScopeFlags;

    struct ThisFinder {
        found: bool,
    }
    impl<'a> Visit<'a> for ThisFinder {
        fn visit_this_expression(&mut self, _: &ThisExpression) {
            self.found = true;
        }
        // Don't recurse into nested functions (they have their own `this`)
        fn visit_function(&mut self, _: &Function<'a>, _flags: ScopeFlags) {}
        fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
            // Arrow functions inherit `this`, so DO recurse
            for stmt in &arrow.body.statements {
                self.visit_statement(stmt);
            }
        }
    }

    for prop in &obj.properties {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            if p.method {
                // It's an object method shorthand like bar() {}
                if let Expression::FunctionExpression(func) = &p.value {
                    let mut finder = ThisFinder { found: false };
                    if let Some(body) = &func.body {
                        for stmt in &body.statements {
                            finder.visit_statement(stmt);
                        }
                    }
                    if finder.found {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Add __workletContextObject: true marker to an object expression.
fn add_context_object_marker<'a>(obj: &mut ObjectExpression<'a>, ast: &AstBuilder<'a>) {
    // Check if already has marker
    let has_marker = obj.properties.iter().any(|prop| {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            if let PropertyKey::StaticIdentifier(id) = &p.key {
                return id.name == CONTEXT_OBJECT_MARKER;
            }
        }
        false
    });
    if has_marker {
        return;
    }
    obj.properties
        .push(ast.object_property_kind_object_property(
            SPAN,
            PropertyKind::Init,
            ast.property_key_static_identifier(SPAN, CONTEXT_OBJECT_MARKER),
            ast.expression_boolean_literal(SPAN, true),
            false,
            false,
            false,
        ));
}

// --- Worklet Class processing ---

/// Check if a class has the __workletClass marker property.
fn is_worklet_class(class: &Class) -> bool {
    class.body.body.iter().any(|element| {
        if let ClassElement::PropertyDefinition(prop) = element {
            if let PropertyKey::StaticIdentifier(id) = &prop.key {
                return id.name == WORKLET_CLASS_MARKER;
            }
        }
        false
    })
}

/// Remove the __workletClass marker from a class body.
fn remove_worklet_class_marker(class: &mut Class) {
    class.body.body.retain(|element| {
        if let ClassElement::PropertyDefinition(prop) = element {
            if let PropertyKey::StaticIdentifier(id) = &prop.key {
                return id.name != WORKLET_CLASS_MARKER;
            }
        }
        true
    });
}

/// Add __workletClass marker to a class (used by file-level worklet directive).
fn add_worklet_class_marker<'a>(class: &mut Class<'a>, ast: &AstBuilder<'a>) {
    // Check if already has marker
    if is_worklet_class(class) {
        return;
    }
    class.body.body.push(ast.class_element_property_definition(
        SPAN,
        PropertyDefinitionType::PropertyDefinition,
        ast.vec(),
        ast.property_key_static_identifier(SPAN, WORKLET_CLASS_MARKER),
        None::<TSTypeAnnotation>,
        Some(ast.expression_boolean_literal(SPAN, true)),
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        None::<TSAccessibility>,
    ));
}

/// Transform a worklet class into a factory function + const declaration.
///
/// Input:
/// ```js
/// class Foo { __workletClass = true; method() {} }
/// ```
///
/// Output:
/// ```js
/// function Foo__classFactory() {
///     'worklet';
///     class Foo { method() {} }
///     Foo.__classFactory = Foo__classFactory;
///     return Foo;
/// }
/// const Foo = Foo__classFactory();
/// ```
fn transform_worklet_class<'a>(
    class: &mut Class<'a>,
    _stmt_idx: usize,
    ctx: &mut WorkletsVisitor<'a>,
) -> Result<Vec<Statement<'a>>, WorkletsError> {
    let class_name: &'a str = class
        .id
        .as_ref()
        .map(|id| id.name.as_str())
        .ok_or_else(|| WorkletsError("Worklet class must have a name".into()))?;

    let factory_name_string = format!("{}{}", class_name, WORKLET_CLASS_FACTORY_SUFFIX);
    let factory_name: &'a str = ctx.allocator.alloc_str(&factory_name_string);

    let ast = AstBuilder::new(ctx.allocator);

    // Remove __workletClass marker
    remove_worklet_class_marker(class);

    // Clone the class into a new class declaration statement
    let class_clone = class.clone_in(ctx.allocator);
    let class_decl = Statement::ClassDeclaration(ast.alloc(class_clone));

    // ClassName.__classFactory = ClassName__classFactory;
    let assign_factory = ast.statement_expression(
        SPAN,
        ast.expression_assignment(
            SPAN,
            AssignmentOperator::Assign,
            AssignmentTarget::StaticMemberExpression(ast.alloc(ast.static_member_expression(
                SPAN,
                ast.expression_identifier(SPAN, class_name),
                ast.identifier_name(SPAN, WORKLET_CLASS_FACTORY_SUFFIX),
                false,
            ))),
            ast.expression_identifier(SPAN, factory_name),
        ),
    );

    // return ClassName;
    let return_stmt = ast.statement_return(SPAN, Some(ast.expression_identifier(SPAN, class_name)));

    // Factory function body with 'worklet' directive
    let factory_body = ast.function_body(
        SPAN,
        ast.vec1(ast.directive(SPAN, ast.string_literal(SPAN, "worklet", None), "worklet")),
        ast.vec_from_iter([class_decl, assign_factory, return_stmt]),
    );

    let factory_func = ast.function(
        SPAN,
        FunctionType::FunctionDeclaration,
        Some(ast.binding_identifier(SPAN, factory_name)),
        false,
        false,
        false,
        None::<TSTypeParameterDeclaration>,
        None::<TSThisParameter>,
        ast.formal_parameters(
            SPAN,
            FormalParameterKind::FormalParameter,
            ast.vec(),
            None::<FormalParameterRest>,
        ),
        None::<TSTypeAnnotation>,
        Some(factory_body),
    );

    let factory_decl = Statement::FunctionDeclaration(ast.alloc(factory_func));

    // const ClassName = ClassName__classFactory();
    let call_factory = ast.call_expression(
        SPAN,
        ast.expression_identifier(SPAN, factory_name),
        None::<TSTypeParameterInstantiation>,
        ast.vec(),
        false,
    );
    let const_decl = build_const_declaration(
        &ast,
        class_name,
        Expression::CallExpression(ast.alloc(call_factory)),
    );
    let const_stmt = Statement::VariableDeclaration(ast.alloc(const_decl));

    Ok(vec![factory_decl, const_stmt])
}

/// For non-context objects in file-level worklet: add 'worklet' to each method body
/// and recursively process property values.
fn process_worklet_aggregator_object<'a>(
    obj: &mut ObjectExpression<'a>,
    ast: &AstBuilder<'a>,
    allocator: &'a Allocator,
) {
    for prop in obj.properties.iter_mut() {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            if p.method {
                // Object method shorthand: add 'worklet' directive to function body
                if let Expression::FunctionExpression(func) = &mut p.value {
                    add_worklet_directive_to_func_body(func, ast);
                }
            } else {
                // Regular property: process value
                add_worklet_directive_to_expr(&mut p.value, ast, allocator);
            }
        }
    }
}
