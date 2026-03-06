/// Builds the minified worklet code string.
///
/// The result looks like:
/// `function workletName(params){const {x,y}=this.__closure;const funcName=this._recur;body}`
pub fn build_worklet_string(
    worklet_name: &str,
    params_code: &str,
    body_code: &str,
    closure_variables: &[String],
    is_async: bool,
    is_generator: bool,
    has_recursive_calls: bool,
    original_name: Option<&str>,
) -> String {
    let mut result = String::new();

    if is_async {
        result.push_str("async ");
    }

    result.push_str("function ");
    if is_generator {
        result.push('*');
    }
    result.push_str(worklet_name);
    result.push('(');
    result.push_str(params_code);
    result.push_str("){");

    // Prepend closure destructuring
    if !closure_variables.is_empty() {
        result.push_str("const{");
        for (i, var) in closure_variables.iter().enumerate() {
            if i > 0 {
                result.push(',');
            }
            result.push_str(var);
        }
        result.push_str("}=this.__closure;");
    }

    // Prepend recursive call restoration
    if has_recursive_calls {
        if let Some(name) = original_name {
            result.push_str("const ");
            result.push_str(name);
            result.push_str("=this._recur;");
        }
    }

    result.push_str(body_code);
    result.push('}');

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_worklet_string_simple() {
        let result = build_worklet_string("foo_null1", "", "return 1;", &[], false, false, false, Some("foo"));
        assert_eq!(result, "function foo_null1(){return 1;}");
    }

    #[test]
    fn test_build_worklet_string_with_closure() {
        let result = build_worklet_string(
            "foo_null1",
            "x",
            "return x+a;",
            &["a".to_string()],
            false,
            false,
            false,
            Some("foo"),
        );
        assert_eq!(result, "function foo_null1(x){const{a}=this.__closure;return x+a;}");
    }

    #[test]
    fn test_build_worklet_string_with_recursion() {
        let result = build_worklet_string(
            "foo_null1",
            "n",
            "return n<=1?1:n*foo(n-1);",
            &[],
            false,
            false,
            true,
            Some("foo"),
        );
        assert_eq!(
            result,
            "function foo_null1(n){const foo=this._recur;return n<=1?1:n*foo(n-1);}"
        );
    }

    #[test]
    fn test_build_worklet_string_async() {
        let result = build_worklet_string("foo_null1", "", "await bar();", &[], true, false, false, Some("foo"));
        assert_eq!(result, "async function foo_null1(){await bar();}");
    }
}
