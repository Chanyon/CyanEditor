use std::{ fs, env, path::PathBuf, io::{ self, Write } };

use crossterm::style::Color;

use crate::{syntax_struct::SyntaxHighlight, Output};

pub const TAB_STOP: usize = 8;
pub struct EditorRows {
    pub row_contents: Vec<Row>,
    pub filename: Option<PathBuf>,
}
impl EditorRows {
    pub fn new(syntax_highlight: &mut Option<Box<dyn SyntaxHighlight>>) -> Self {
        let mut arg = env::args();
        match arg.nth(1) {
            Some(file_path) => Self::from_file(file_path.into(), syntax_highlight),
            None => Self { row_contents: Vec::new(), filename: None },
        }
    }
    pub fn number_of_rows(&self) -> usize {
        self.row_contents.len()
    }
    // pub fn get_row_str(&self, at: usize) -> &str {
    //     &self.row_contents[at].render
    // }
    pub fn get_render_row(&self, at: usize) -> &String {
        &self.row_contents[at].render
    }
    pub fn get_editor_row(&self, at: usize) -> &Row {
        &self.row_contents[at]
    }
    fn from_file(file_path: PathBuf, syntax_highlight: &mut Option<Box<dyn SyntaxHighlight>>) -> Self {
        let file_content = fs::read_to_string(&file_path).expect("Unable to read file.");
        let mut row_contents = Vec::new();

        file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| Output::select_syntax(ext)
            .map(|syntax| syntax_highlight.insert(syntax)));

        file_content
            .lines()
            .enumerate()
            .for_each(|(i, line)| {
                let mut row = Row::new(line.into(), String::new());
                EditorRows::render_row(&mut row);
                row_contents.push(row);
                if let Some(it) = syntax_highlight {
                    it.update_syntax(i, &mut row_contents);
                }
            });
        Self { row_contents, filename: Some(file_path) }
    }
    pub fn render_row(row: &mut Row) {
        let mut index = 0;
        let capacity = row.row_content
            .chars()
            .fold(0, |acc, next| acc + (if next == '\t' { 8 } else { 1 }));
        row.render = String::with_capacity(capacity);
        row.row_content.chars().for_each(|c| {
            index += 1;
            if c == '\t' {
                // 用空格代替tab
                row.render.push(' ');
                while index % TAB_STOP != 0 {
                    row.render.push(' ');
                    index += 1;
                }
            } else {
                row.render.push(c);
            }
        });
    }
    pub fn insert_row(&mut self, at: usize, contents: String) {
        let mut new_row = Row::new(contents, String::new());
        EditorRows::render_row(&mut new_row);
        self.row_contents.insert(at, new_row);
    }
    pub fn get_editor_row_mut(&mut self, at: usize) -> &mut Row {
        &mut self.row_contents[at]
    }
    pub fn save(&self) -> io::Result<usize> {
        match &self.filename {
            Some(name) => {
                let mut file = fs::OpenOptions::new().write(true).create(true).open(name)?;
                let contents = self.row_contents
                    .iter()
                    .map(|item| item.row_content.as_str())
                    .collect::<Vec<&str>>()
                    .join("\n");
                let contents_u8 = contents.as_bytes();
                file.set_len(contents.len() as u64)?;
                file.write_all(contents_u8)?;
                Ok(contents_u8.len())
            }
            None => Err(io::Error::new(io::ErrorKind::Other, "no file")),
        }
    }
    pub fn join_adjacent_rows(&mut self, at: usize) {
        let current_row = self.row_contents.remove(at);
        let previous_row = self.get_editor_row_mut(at - 1);
        previous_row.row_content.push_str(&current_row.row_content);
        Self::render_row(previous_row);
    }
}

// Tabs
#[derive(Clone)]
pub struct Row {
    pub row_content: String,
    pub render: String,
    pub highlight: Vec<HighlightType>,
    pub is_comment: bool,
}

impl Row {
    pub fn new(row_content: String, render: String) -> Self {
        Self {
            row_content,
            render,
            highlight: Vec::new(),
            is_comment: false,
        }
    }
    pub fn insert_char(&mut self, at: usize, ch: char) {
        self.row_content.insert(at, ch);
        EditorRows::render_row(self);
    }
    pub fn delete_char(&mut self, at: usize) {
        self.row_content.remove(at);
        EditorRows::render_row(self)
    }
    // 处理比较长的行
    pub fn get_row_content_x(&self, render_x: usize) -> usize {
        let mut current_row_x = 0;
        for (cursor_x, ch) in self.row_content.chars().enumerate() {
            if ch == '\t' {
                current_row_x += TAB_STOP - 1 - (current_row_x % TAB_STOP);
            }
            current_row_x += 1;
            if current_row_x > render_x {
                return cursor_x;
            }
        }
        0
    }
}

#[derive(Clone,Copy)]
pub enum HighlightType {
    Normal,
    Number,
    SearchMatch,
    String,
    CharLiteral,
    Comment,
    MultilineComment,
    Other(Color),
}