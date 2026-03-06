use std::collections::HashSet;

use oxc::ast::ast::*;
use oxc::ast_visit::Visit;
use oxc::syntax::scope::ScopeFlags;

/// Collects closure variables from a function body.
pub fn get_closure(
    func: &Function<'_>,
    globals: &HashSet<String>,
    strict_global: bool,
) -> Vec<String> {
    let mut collector = ClosureCollector {
        globals,
        strict_global,
        captured: Vec::new(),
        captured_names: HashSet::new(),
        local_bindings: Vec::new(),
        func_name: func.id.as_ref().map(|id| id.name.to_string()),
    };

    collector.push_scope();
    collector.collect_params(&func.params);
    if let Some(body) = &func.body {
        collector.collect_block_bindings_from_stmts(&body.statements);
        for stmt in &body.statements {
            collector.visit_statement(stmt);
        }
    }
    collector.pop_scope();

    collector.captured
}

/// Collects closure variables from an arrow function expression body.
pub fn get_closure_arrow(
    arrow: &ArrowFunctionExpression<'_>,
    globals: &HashSet<String>,
    strict_global: bool,
) -> Vec<String> {
    let mut collector = ClosureCollector {
        globals,
        strict_global,
        captured: Vec::new(),
        captured_names: HashSet::new(),
        local_bindings: Vec::new(),
        func_name: None,
    };

    collector.push_scope();
    collector.collect_params(&arrow.params);
    collector.collect_block_bindings_from_stmts(&arrow.body.statements);
    for stmt in &arrow.body.statements {
        collector.visit_statement(stmt);
    }
    collector.pop_scope();

    collector.captured
}

struct ClosureCollector<'g> {
    globals: &'g HashSet<String>,
    strict_global: bool,
    captured: Vec<String>,
    captured_names: HashSet<String>,
    local_bindings: Vec<HashSet<String>>,
    func_name: Option<String>,
}

impl<'g> ClosureCollector<'g> {
    fn push_scope(&mut self) {
        self.local_bindings.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.local_bindings.pop();
    }

    fn add_local(&mut self, name: &str) {
        if let Some(scope) = self.local_bindings.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.local_bindings.iter().any(|scope| scope.contains(name))
    }

    fn try_capture(&mut self, name: &str) {
        if self.captured_names.contains(name) {
            return;
        }
        if self.is_local(name) {
            return;
        }
        if let Some(ref func_name) = self.func_name {
            if name == func_name {
                return;
            }
        }
        if self.strict_global || self.globals.contains(name) {
            return;
        }
        self.captured_names.insert(name.to_string());
        self.captured.push(name.to_string());
    }

    fn collect_params(&mut self, params: &FormalParameters) {
        for param in &params.items {
            self.collect_binding_pattern(&param.pattern);
        }
        if let Some(rest) = &params.rest {
            self.collect_binding_pattern(&rest.rest.argument);
        }
    }

    fn collect_binding_pattern(&mut self, pattern: &BindingPattern) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.add_local(id.name.as_str());
            }
            BindingPattern::ObjectPattern(obj) => {
                for prop in &obj.properties {
                    self.collect_binding_pattern(&prop.value);
                }
                if let Some(rest) = &obj.rest {
                    self.collect_binding_pattern(&rest.argument);
                }
            }
            BindingPattern::ArrayPattern(arr) => {
                for elem in arr.elements.iter().flatten() {
                    self.collect_binding_pattern(elem);
                }
                if let Some(rest) = &arr.rest {
                    self.collect_binding_pattern(&rest.argument);
                }
            }
            BindingPattern::AssignmentPattern(assign) => {
                self.collect_binding_pattern(&assign.left);
            }
        }
    }

    fn collect_block_bindings_from_stmts(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            match stmt {
                Statement::VariableDeclaration(decl) => {
                    for declarator in &decl.declarations {
                        self.collect_binding_pattern(&declarator.id);
                    }
                }
                Statement::FunctionDeclaration(func) => {
                    if let Some(id) = &func.id {
                        self.add_local(id.name.as_str());
                    }
                }
                Statement::ClassDeclaration(class) => {
                    if let Some(id) = &class.id {
                        self.add_local(id.name.as_str());
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_block_scoped_bindings(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            match stmt {
                Statement::VariableDeclaration(decl)
                    if matches!(
                        decl.kind,
                        VariableDeclarationKind::Let | VariableDeclarationKind::Const
                    ) =>
                {
                    for declarator in &decl.declarations {
                        self.collect_binding_pattern(&declarator.id);
                    }
                }
                Statement::FunctionDeclaration(func) => {
                    if let Some(id) = &func.id {
                        self.add_local(id.name.as_str());
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'a, 'g> Visit<'a> for ClosureCollector<'g> {
    fn visit_identifier_reference(&mut self, ident: &IdentifierReference<'a>) {
        self.try_capture(ident.name.as_str());
    }

    fn visit_function(&mut self, func: &Function<'a>, _flags: ScopeFlags) {
        if has_worklet_directive_body(func.body.as_ref()) {
            return;
        }
        self.push_scope();
        if let Some(id) = &func.id {
            self.add_local(id.name.as_str());
        }
        self.collect_params(&func.params);
        if let Some(body) = &func.body {
            self.collect_block_bindings_from_stmts(&body.statements);
            for stmt in &body.statements {
                self.visit_statement(stmt);
            }
        }
        self.pop_scope();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        if has_worklet_directive_fn_body(&arrow.body) {
            return;
        }
        self.push_scope();
        self.collect_params(&arrow.params);
        self.collect_block_bindings_from_stmts(&arrow.body.statements);
        for stmt in &arrow.body.statements {
            self.visit_statement(stmt);
        }
        self.pop_scope();
    }

    fn visit_block_statement(&mut self, block: &BlockStatement<'a>) {
        self.push_scope();
        self.collect_block_scoped_bindings(&block.body);
        for stmt in &block.body {
            self.visit_statement(stmt);
        }
        self.pop_scope();
    }

    fn visit_for_statement(&mut self, stmt: &ForStatement<'a>) {
        self.push_scope();
        if let Some(init) = &stmt.init {
            match init {
                ForStatementInit::VariableDeclaration(decl) => {
                    for d in &decl.declarations {
                        self.collect_binding_pattern(&d.id);
                        if let Some(init) = &d.init {
                            self.visit_expression(init);
                        }
                    }
                }
                _ => {
                    self.visit_for_statement_init(init);
                }
            }
        }
        if let Some(test) = &stmt.test {
            self.visit_expression(test);
        }
        if let Some(update) = &stmt.update {
            self.visit_expression(update);
        }
        self.visit_statement(&stmt.body);
        self.pop_scope();
    }

    fn visit_for_in_statement(&mut self, stmt: &ForInStatement<'a>) {
        self.push_scope();
        if let ForStatementLeft::VariableDeclaration(decl) = &stmt.left {
            for d in &decl.declarations {
                self.collect_binding_pattern(&d.id);
            }
        } else {
            self.visit_for_statement_left(&stmt.left);
        }
        self.visit_expression(&stmt.right);
        self.visit_statement(&stmt.body);
        self.pop_scope();
    }

    fn visit_for_of_statement(&mut self, stmt: &ForOfStatement<'a>) {
        self.push_scope();
        if let ForStatementLeft::VariableDeclaration(decl) = &stmt.left {
            for d in &decl.declarations {
                self.collect_binding_pattern(&d.id);
            }
        } else {
            self.visit_for_statement_left(&stmt.left);
        }
        self.visit_expression(&stmt.right);
        self.visit_statement(&stmt.body);
        self.pop_scope();
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause<'a>) {
        self.push_scope();
        if let Some(param) = &clause.param {
            self.collect_binding_pattern(&param.pattern);
        }
        self.collect_block_scoped_bindings(&clause.body.body);
        for stmt in &clause.body.body {
            self.visit_statement(stmt);
        }
        self.pop_scope();
    }

    fn visit_ts_type(&mut self, _ty: &TSType<'a>) {}
    fn visit_ts_type_alias_declaration(&mut self, _decl: &TSTypeAliasDeclaration<'a>) {}
    fn visit_ts_interface_declaration(&mut self, _decl: &TSInterfaceDeclaration<'a>) {}

    fn visit_member_expression(&mut self, expr: &MemberExpression<'a>) {
        match expr {
            MemberExpression::StaticMemberExpression(member) => {
                self.visit_expression(&member.object);
            }
            MemberExpression::ComputedMemberExpression(member) => {
                self.visit_expression(&member.object);
                self.visit_expression(&member.expression);
            }
            MemberExpression::PrivateFieldExpression(member) => {
                self.visit_expression(&member.object);
            }
        }
    }

    fn visit_object_property(&mut self, prop: &ObjectProperty<'a>) {
        if prop.computed {
            self.visit_property_key(&prop.key);
        }
        self.visit_expression(&prop.value);
    }
}

fn has_worklet_directive_body<'a>(
    body: Option<&oxc::allocator::Box<'a, FunctionBody<'a>>>,
) -> bool {
    body.is_some_and(|b| has_worklet_directive_fn_body(b))
}

fn has_worklet_directive_fn_body(body: &FunctionBody) -> bool {
    body.directives
        .iter()
        .any(|d| d.expression.value == "worklet")
}
