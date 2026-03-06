use std::path::Path;

pub fn hash(s: &str) -> u64 {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    let mut hash1: u32 = 5381;
    let mut hash2: u32 = 52711;

    while i > 0 {
        i -= 1;
        let c = bytes[i] as u32;
        hash1 = hash1.wrapping_mul(33) ^ c;
        hash2 = hash2.wrapping_mul(33) ^ c;
    }

    (hash1 as u64) * 4096 + (hash2 as u64)
}

pub fn make_worklet_name(
    func_name: Option<&str>,
    filename: &str,
    worklet_number: u32,
) -> (String, String) {
    let mut source = "unknownFile".to_string();

    if !filename.is_empty() {
        let path = Path::new(filename);
        source = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknownFile".to_string());

        // Get library name from path if it's in node_modules
        let components: Vec<&str> = filename.split('/').collect();
        if let Some(idx) = components.iter().position(|&c| c == "node_modules") {
            if idx + 1 < components.len() {
                let library_name = components[idx + 1];
                source = format!("{}_{}", library_name, source);
            }
        }
    }

    let suffix = format!("{}{}", source, worklet_number);
    let mut react_name = String::new();

    if let Some(name) = func_name {
        if !name.is_empty() {
            react_name = name.to_string();
        }
    }

    let worklet_name = if !react_name.is_empty() {
        to_identifier(&format!("{}_{}", react_name, suffix))
    } else {
        to_identifier(&suffix)
    };

    // Fallback for arrow functions and unnamed function expressions
    if react_name.is_empty() {
        react_name = to_identifier(&suffix);
    }

    (worklet_name, react_name)
}

/// Converts a string to a valid JavaScript identifier.
fn to_identifier(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            result.push(c);
        } else if c == '.' || c == '-' || c == ' ' {
            // Replace common separator characters with underscore
            result.push('_');
        }
        // Skip other invalid characters
    }
    // Ensure it doesn't start with a digit
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    if result.is_empty() {
        result.push_str("_unnamed");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash() {
        // Test the hash function produces consistent results
        let h = hash("function foo(){return 1;}");
        assert!(h > 0);
        // Same input should produce same hash
        assert_eq!(h, hash("function foo(){return 1;}"));
        // Different input should produce different hash
        assert_ne!(h, hash("function bar(){return 2;}"));
    }

    #[test]
    fn test_to_identifier() {
        assert_eq!(to_identifier("hello"), "hello");
        assert_eq!(to_identifier("hello-world"), "hello_world");
        assert_eq!(to_identifier("123abc"), "_123abc");
        assert_eq!(to_identifier("hello.ts"), "hello_ts");
    }

    #[test]
    fn test_make_worklet_name() {
        let (worklet_name, react_name) = make_worklet_name(Some("foo"), "/dev/null", 1);
        assert_eq!(worklet_name, "foo_null1");
        assert_eq!(react_name, "foo");

        let (worklet_name, react_name) = make_worklet_name(None, "/dev/null", 1);
        assert_eq!(worklet_name, "null1");
        assert_eq!(react_name, "null1");
    }
}
