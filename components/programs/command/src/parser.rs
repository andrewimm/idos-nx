use core::sync::atomic::{AtomicU32, Ordering};

use super::lexer::{Lexer, Token};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

pub struct Parser<'input> {
    lexer: Lexer<'input>,
    tree: CommandTree,
    current: Token,
    next: Token,
}

impl<'input> Parser<'input> {
    pub fn new(lexer: Lexer<'input>) -> Self {
        let mut parser = Self {
            lexer,
            tree: CommandTree::new(),
            current: Token::Invalid,
            next: Token::Invalid,
        };

        // fill the token slots
        parser.advance_token();
        parser.advance_token();

        parser
    }

    pub fn advance_token(&mut self) -> Token {
        let next = self.lexer.next_token();
        let cur = core::mem::replace(&mut self.next, next);
        core::mem::replace(&mut self.current, cur)
    }

    pub fn parse_input(&mut self) {
        let mut segment_ids = Vec::new();
        loop {
            match self.current {
                Token::End => break,
                Token::Invalid => break,
                Token::RedirectInput => {
                    let _source = self.parse_filename();

                    panic!("not implemented");
                }
                _ => {
                    let next_segment = self.parse_segment();
                    let id = self.tree.insert(next_segment);
                    segment_ids.push(id);
                }
            }
        }
    }

    pub fn parse_segment(&mut self) -> CommandComponent {
        match &self.current {
            Token::Argument(bytes) => {
                // first Argument is a command, or possibly a drive switch
                let name_str = core::str::from_utf8(&bytes).unwrap();
                let name = String::from(name_str);

                self.advance_token();
                let arguments = self.parse_argument_string();

                CommandComponent::Executable(name, arguments)
            }
            _ => unimplemented!(),
        }
    }

    pub fn parse_argument_string(&mut self) -> Vec<String> {
        let mut arguments = Vec::new();

        while let Token::Argument(bytes) = &self.current {
            let arg_str = core::str::from_utf8(&bytes).unwrap();
            let arg = String::from(arg_str);
            arguments.push(arg);
            self.advance_token();
        }

        arguments
    }

    pub fn parse_filename(&mut self) -> CommandComponent {
        panic!("")
    }

    pub fn into_tree(self) -> CommandTree {
        self.tree
    }
}

pub enum CommandComponent {
    ChangeDrive(String),
    Executable(String, Vec<String>),
    Filename(String),
}

pub type ComponentID = u32;

pub struct CommandTree {
    root: ComponentID,
    next_id: AtomicU32,
    tree: BTreeMap<ComponentID, CommandComponent>,
}

impl CommandTree {
    pub fn new() -> Self {
        Self {
            root: 0,
            next_id: AtomicU32::new(0),
            tree: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, component: CommandComponent) -> ComponentID {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.tree.insert(id, component);
        id
    }

    pub fn len(&self) -> usize {
        self.tree.len()
    }

    pub fn get_root(&self) -> Option<&CommandComponent> {
        self.tree.get(&self.root)
    }
}
