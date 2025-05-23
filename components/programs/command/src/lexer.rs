use alloc::vec::Vec;

pub struct Lexer<'input> {
    input: &'input [u8],
    current_pos: usize,
}

impl<'input> Lexer<'input> {
    pub fn new(input: &'input [u8]) -> Self {
        Self {
            input,
            current_pos: 0,
        }
    }

    pub fn peek_next_char(&self) -> Option<u8> {
        if self.current_pos >= self.input.len() {
            return None;
        }
        Some(self.input[self.current_pos])
    }

    pub fn next_token(&mut self) -> Token {
        self.eat_whitespace();

        let cur = match self.peek_next_char() {
            None => return Token::End,
            Some(c) => c,
        };

        match cur {
            b'|' => {
                self.current_pos += 1;
                return Token::Pipe;
            }
            b'<' => {
                self.current_pos += 1;
                return Token::RedirectInput;
            }
            b'>' => {
                self.current_pos += 1;
                if self.peek_next_char() == Some(b'>') {
                    return Token::RedirectOutputAppend;
                } else {
                    return Token::RedirectOutputOverwrite;
                }
            }
            _ => (),
        }

        self.get_argument()
    }

    pub fn eat_whitespace(&mut self) {
        while let Some(cur) = self.peek_next_char() {
            if is_whitespace(cur) {
                self.current_pos += 1;
            } else {
                return;
            }
        }
    }

    pub fn get_argument(&mut self) -> Token {
        let mut bytes = Vec::new();
        while let Some(cur) = self.peek_next_char() {
            if is_whitespace(cur) {
                break;
            }
            bytes.push(cur);
            self.current_pos += 1;
        }
        Token::Argument(bytes)
    }
}

fn is_whitespace(ch: u8) -> bool {
    match ch {
        b' ' => true,
        b'\t' => true,
        b'\n' => true,
        b'\r' => true,
        _ => false,
    }
}

pub enum Token {
    Argument(Vec<u8>),
    RedirectInput,
    RedirectOutputOverwrite,
    RedirectOutputAppend,
    Pipe,
    Invalid,
    End,
}
