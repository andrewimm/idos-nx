use alloc::string::String;

/// Path stores a mutable, combinable path to a file using a heap-allocated
/// String as the underlying storage
#[derive(Clone)]
pub struct Path {
    inner: String,
}

impl Path {
    pub fn from_str(s: &str) -> Self {
        let sanitized = s.trim_end_matches('\\').trim_start_matches('\\');
        Self {
            inner: String::from(sanitized),
        }
    }

    pub fn is_absolute(s: &str) -> bool {
        let mut colon_index: Option<usize> = None;
        for (index, ch) in s.char_indices() {
            match ch {
                'a'..='z' | 'A'..='Z' => continue,
                ':' => {
                    colon_index = Some(index);
                },
                '\\' => {
                    if let Some(i) = colon_index {
                        return (i + 1) == index && i > 0;
                    }
                    return false;
                }
                _ => return false,
            }
        }
        return false;
    }

    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    pub fn push(&mut self, to_append: &str) {
        for element in to_append.split('\\') {
            match element {
                "" | "." => (),
                ".." => self.pop(),
                _ => {
                    if !self.inner.is_empty() {
                        self.inner.push('\\');
                    }
                    self.inner.push_str(element);
                },
            }
        }
    }

    pub fn pop(&mut self) {
        let mut last_instance = None;
        for (index, ch) in self.inner.char_indices() {
            if ch == '\\' {
                last_instance = Some(index);
            }
        }
        match last_instance {
            Some(index) => self.inner.truncate(index),
            None => self.inner.truncate(0),
        }
    }
}

impl Into<String> for Path {
    fn into(self) -> String {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::Path;

    #[test_case]
    fn push() {
        let mut path = Path::from_str("abc\\defghi\\j");
        path.push("klmn");
        path.push("x\\yy\\zzz");
        assert_eq!(path.as_str(), "abc\\defghi\\j\\klmn\\x\\yy\\zzz");
        path.push("..\\..\\..");
        assert_eq!(path.as_str(), "abc\\defghi\\j\\klmn");
        path.push(".");
        assert_eq!(path.as_str(), "abc\\defghi\\j\\klmn");
        path.push("\\\\\\");
        assert_eq!(path.as_str(), "abc\\defghi\\j\\klmn");
    }

    #[test_case]
    fn pop() {
        let mut path = Path::from_str("path\\to\\the\\file.txt");
        path.pop();
        assert_eq!(path.as_str(), "path\\to\\the");
        path.pop();
        assert_eq!(path.as_str(), "path\\to");
        path.pop();
        assert_eq!(path.as_str(), "path");
        path.pop();
        assert_eq!(path.as_str(), "");
        path.pop();
        assert_eq!(path.as_str(), "");
    }

    #[test_case]
    fn is_absolute() {
        assert!(Path::is_absolute("C:\\DOS"));
        assert!(!Path::is_absolute("\\Directory"));
        assert!(!Path::is_absolute("D"));
        assert!(!Path::is_absolute(":\\"));
        assert!(!Path::is_absolute("123:\\"));
        assert!(Path::is_absolute("ABC:\\"));
    }
}

