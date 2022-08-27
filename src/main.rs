use std::io::{ self, stdout, Write };
use std::cmp;
use std::path::PathBuf;
use std::time::Duration;
use crossterm::terminal::ClearType;
use crossterm::{ event, terminal, execute, cursor, queue, style::{ self, Color } };
use crossterm::event::{ Event, KeyCode, KeyEvent };

mod cursor_xy;
mod editor_row;
mod status_message;
mod prompt;
mod search_direction;
mod syntax_struct;

use cursor_xy::CursorController;
use editor_row::EditorRows;
use status_message::StatusMessage;
use search_direction::*;

use crate::syntax_struct::SyntaxHighlight;
use crate::editor_row::HighlightType;
use crate::editor_row::Row;

const VERSION: &'static str = "0.1.0";
struct CleanUp;

impl Drop for CleanUp {
    fn drop(&mut self) {
        terminal::disable_raw_mode().expect("Could not disable raw mode");
        Output::clear_screen().expect("Error");
    }
}

struct Reader; //read keypress

impl Reader {
    fn read_key(&self) -> crossterm::Result<KeyEvent> {
        loop {
            if event::poll(Duration::from_millis(500))? {
                if let Event::Key(event) = event::read()? {
                    return Ok(event);
                }
            }
        }
    }
}

struct Editor {
    reader: Reader,
    output: Output,
    quit_time: u8,
}

impl Editor {
    fn new() -> Self {
        Self {
            reader: Reader,
            output: Output::new(),
            quit_time: 2,
        }
    }

    fn process_keypress(&mut self) -> crossterm::Result<bool> {
        match self.reader.read_key()? {
            KeyEvent { code: KeyCode::Char('q'), modifiers: event::KeyModifiers::CONTROL } => {
                if self.output.dirty > 0 && self.quit_time > 0 {
                    self.output.status_message.set_message(
                        format!(
                            "WARING! File unsaved changes. Press Ctrl-Q {} more times to quit. ",
                            self.quit_time
                        )
                    );
                    self.quit_time = self.quit_time.saturating_sub(1);
                    return Ok(true);
                }
                return Ok(false);
            }
            KeyEvent {
                code: code @ (
                    KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::End
                    | KeyCode::Home
                ),
                modifiers: event::KeyModifiers::NONE,
            } => {
                self.output.move_cursor(code);
            }
            KeyEvent {
                code: val @ (KeyCode::PageUp | KeyCode::PageDown),
                modifiers: event::KeyModifiers::NONE,
            } => {
                if matches!(val, KeyCode::PageUp) {
                    self.output.cursor_controller.cursor_y =
                        self.output.cursor_controller.row_offset;
                } else {
                    self.output.cursor_controller.cursor_y = cmp::min(
                        self.output.win_size.1 + self.output.cursor_controller.row_offset - 1,
                        self.output.editor_rows.number_of_rows()
                    );
                }
                let key = if matches!(val, KeyCode::PageUp) { KeyCode::Up } else { KeyCode::Down };
                (0..self.output.win_size.1).for_each(|_| {
                    self.output.move_cursor(key);
                });
            }
            KeyEvent { code: KeyCode::Char('s'), modifiers: event::KeyModifiers::CONTROL } => {
                if matches!(self.output.editor_rows.filename, None) {
                    let prompt: Option<PathBuf> = prompt!(
                        &mut self.output,
                        "Save as : {} (esc to cancel)"
                    ).map(|item| item.into());
                    if let None = prompt {
                        self.output.status_message.set_message("Save Aborted".to_string());
                        return Ok(true);
                    }
                    prompt
                        .as_ref()
                        .and_then(|path| path.extension())
                        .and_then(|ext| ext.to_str())
                        .map(|ext| {
                            Output::select_syntax(ext).map(|syntax| {
                                let highlight = self.output.syntax_highlight.insert(syntax);
                                for i in 0..self.output.editor_rows.number_of_rows() {
                                    highlight.update_syntax(i, &mut self.output.editor_rows.row_contents);
                                }
                            });
                        });
                    self.output.editor_rows.filename = prompt;
                }
                self.output.editor_rows.save().map(|len| {
                    self.output.status_message.set_message(
                        format!("{} bytes written to disk.", len)
                    );
                    self.output.dirty = 0;
                })?;
            }
            KeyEvent {
                code: code @ (KeyCode::Char(..) | KeyCode::Tab),
                modifiers: event::KeyModifiers::NONE | event::KeyModifiers::SHIFT,
            } => {
                self.output.inset_char(match code {
                    KeyCode::Tab => '\t',
                    KeyCode::Char(ch) => ch,
                    _ => unreachable!(),
                });
            }
            KeyEvent {
                code: key @ (KeyCode::Backspace | KeyCode::Delete),
                modifiers: event::KeyModifiers::NONE,
            } => {
                if matches!(key, KeyCode::Delete) {
                    self.output.move_cursor(KeyCode::Right);
                }
                self.output.delete_char();
            }
            KeyEvent { code: KeyCode::Enter, modifiers: event::KeyModifiers::NONE } => {
                self.output.insert_newline();
            }
            KeyEvent { code: KeyCode::Char('f'), modifiers: event::KeyModifiers::CONTROL } => {
                self.output.find()?;
            }
            _ => {}
        }
        Ok(true)
    }
    fn run(&mut self) -> crossterm::Result<bool> {
        self.output.refresh_screen()?;
        self.process_keypress()
    }
}

pub struct EditorContents {
    // print the `~`
    content: String,
}
impl EditorContents {
    fn new() -> Self {
        Self { content: String::new() }
    }
    fn push(&mut self, ch: char) {
        self.content.push(ch);
    }
    fn push_str(&mut self, string: &str) {
        self.content.push_str(string);
    }
}

impl io::Write for EditorContents {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match std::str::from_utf8(buf) {
            Ok(s) => {
                self.content.push_str(s);
                Ok(s.len())
            }
            Err(_) => Err(io::ErrorKind::WriteZero.into()),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        let out = write!(stdout(), "{}", self.content);
        stdout().flush()?;
        self.content.clear();
        out
    }
}

// clear Screen;
struct Output {
    win_size: (usize, usize),
    editor_contents: EditorContents,
    cursor_controller: CursorController,
    editor_rows: EditorRows,
    status_message: StatusMessage,
    dirty: u64, // dirty flag
    search_index: SearchIndex,
    syntax_highlight: Option<Box<dyn SyntaxHighlight>>,
}

syntax_struct! {
    struct RustHighlight {
        extensions: &["rs"],
        file_type: "rust",
        comment_start: "//",
        keywords: {
            [Color::DarkRed;
                "mod","unsafe","extern","crate","use","type","struct","enum","union","const","static",
                "mut","let","if","else","impl","trait","for","fn","self","Self", "while", "true","false",
                "in","continue","break","loop","match"
            ],
            [Color::DarkMagenta;
                "isize","i8","i16","i32","i64","usize","u8","u16","u32","u64","f32","f64",
                "char","str","bool","T","U","R","F","L","S","Fn","FnOnce","FnMut"
            ]
        },
        multiline_comment: Some(("/*","*/"))
    }
}

impl Output {
    fn new() -> Self {
        let win_size = terminal
            ::size()
            .map(|(x, y)| (x as usize, (y as usize) - 2))
            .unwrap();
        let mut syntax_highlight = None;
        Output {
            win_size,
            editor_contents: EditorContents::new(),
            cursor_controller: CursorController::new(win_size),
            editor_rows: EditorRows::new(&mut syntax_highlight),
            status_message: StatusMessage::new(
                "HELP: Ctrl-Q = Quit | Ctrl-s = Save | Ctrl-F = Find".to_string()
            ),
            dirty: 0,
            search_index: SearchIndex::new(),
            syntax_highlight,
        }
    }
    // 绘制文件行
    fn draw_rows(&mut self) {
        let screen_rows = self.win_size.1;
        let screen_columns = self.win_size.0;
        for i in 0..screen_rows {
            let file_row = i + self.cursor_controller.row_offset; // scroll,when self.cursor_controller.row_offset 发生变化时用来读取vec里的字符
            if file_row >= self.editor_rows.number_of_rows() {
                if i == screen_rows / 3 && 0 == self.editor_rows.number_of_rows() {
                    // println welcomes message.
                    let mut welcomes = format!("Pound Editor --- Version {}", VERSION);
                    if welcomes.len() > screen_columns {
                        welcomes.truncate(screen_columns);
                    }
                    // 计算padding
                    let mut padding = (screen_columns - welcomes.len()) / 2;
                    if padding != 0 {
                        self.editor_contents.push(' ');
                        padding -= 1;
                    }
                    (0..padding).for_each(|_| self.editor_contents.push(' '));
                    self.editor_contents.push_str(&welcomes);
                } else {
                    self.editor_contents.push('~');
                }
            } else {
                if i < self.editor_rows.number_of_rows() {
                    // let len = cmp::min(self.editor_rows.get_render_row(file_row).len(), screen_columns);
                    // self.editor_contents.push_str(&self.editor_rows.get_render_row(file_row)[..len]);
                    let row = self.editor_rows.get_editor_row(file_row);
                    let current_row_len = row.render.len();
                    let column_offset = self.cursor_controller.column_offset;
                    let len = if current_row_len < column_offset {
                        0
                    } else {
                        let len = current_row_len - column_offset;
                        if len > screen_columns {
                            screen_columns
                        } else {
                            len
                        }
                    };
                    let start = if len == 0 { 0 } else { column_offset };
                    let render = &row.render[start..start + len];
                    self.syntax_highlight
                        .as_ref()
                        .map(|syntax_struct| {
                            syntax_struct.color_row(
                                render,
                                &row.highlight[start..start + len],
                                &mut self.editor_contents
                            )
                        })
                        .unwrap_or_else(|| self.editor_contents.push_str(render));
                    // self.editor_contents.push_str(&row[start..start + len]);
                }
            }
            queue!(self.editor_contents, terminal::Clear(ClearType::UntilNewLine)).unwrap();
            // if i < screen_rows - 1 {
            self.editor_contents.push_str("\r\n");
            // }
        }
    }
    // tab bar
    fn draw_status_bar(&mut self) {
        self.editor_contents.push_str(&style::Attribute::Reverse.to_string());
        let dirty = if self.dirty > 0 { "(modified)" } else { "" };
        let info = format!(
            "{} {} -- {}lines",
            self.editor_rows.filename
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("[No Name]"),
            dirty,
            self.editor_rows.number_of_rows()
        );
        let info_len = cmp::min(info.len(), self.win_size.0);
        let line_info = format!(
            "{} {}/{}",
            self.syntax_highlight
                .as_ref()
                .map(|high| high.file_type())
                .unwrap_or("no file_type"),
            self.cursor_controller.cursor_y,
            self.editor_rows.number_of_rows()
        );
        self.editor_contents.push_str(&info[..info_len]);

        for idx in info_len..self.win_size.0 {
            // 计算剩余位置
            if self.win_size.0 - idx == line_info.len() {
                self.editor_contents.push_str(&line_info);
                break;
            } else {
                self.editor_contents.push(' ');
            }
        }
        self.editor_contents.push_str(&style::Attribute::Reset.to_string());
        // self.editor_contents.push_str("\r\n");
    }

    fn draw_message_bar(&mut self) {
        queue!(self.editor_contents, terminal::Clear(ClearType::UntilNewLine)).unwrap();
        if let Some(msg) = self.status_message.message() {
            let msg_len = msg.len();
            let min_msg_len = cmp::min(self.win_size.0, msg_len);
            self.editor_contents.push_str(&msg[..min_msg_len]);
        }
        self.editor_contents.push_str("\r\n");
    }

    fn clear_screen() -> crossterm::Result<()> {
        execute!(stdout(), terminal::Clear(ClearType::All))?;
        // 改变游标位置
        execute!(stdout(), cursor::MoveTo(0, 0))
    }

    fn refresh_screen(&mut self) -> crossterm::Result<()> {
        self.cursor_controller.scroll(&self.editor_rows); //窗口垂直、水平滚动
        queue!(
            self.editor_contents,
            cursor::Hide,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;

        self.draw_rows(); //like vim;
        self.draw_message_bar();
        self.draw_status_bar();

        let (cursor_x, cursor_y) = (
            // saturating at the numeric bounds instead of overflowing.
            self.cursor_controller.render_x.saturating_sub(self.cursor_controller.column_offset),
            self.cursor_controller.cursor_y.saturating_sub(self.cursor_controller.row_offset),
        );
        queue!(
            self.editor_contents,
            cursor::MoveTo(cursor_x as u16, cursor_y as u16),
            cursor::Show
        )?;
        // 标准输出
        self.editor_contents.flush()
    }

    fn move_cursor(&mut self, direction: KeyCode) {
        self.cursor_controller.move_cursor(direction, &self.editor_rows);
    }

    fn inset_char(&mut self, ch: char) {
        if self.cursor_controller.cursor_y == self.editor_rows.number_of_rows() {
            self.editor_rows.insert_row(self.editor_rows.number_of_rows(), String::new());
            self.dirty += 1;
        } else {
            self.editor_rows
                .get_editor_row_mut(self.cursor_controller.cursor_y)
                .insert_char(self.cursor_controller.cursor_x, ch);
            if let Some(it) = self.syntax_highlight.as_ref() {
                it.update_syntax(
                    self.cursor_controller.cursor_y,
                    &mut self.editor_rows.row_contents
                );
            }
            self.cursor_controller.cursor_x += 1;
            self.dirty += 1;
        }
    }

    fn delete_char(&mut self) {
        if self.cursor_controller.cursor_y == self.editor_rows.number_of_rows() {
            return;
        }
        if self.cursor_controller.cursor_y == 0 && self.cursor_controller.cursor_x == 0 {
            return;
        }
        let row = self.editor_rows.get_editor_row_mut(self.cursor_controller.cursor_y);
        if self.cursor_controller.cursor_x > 0 {
            row.delete_char(self.cursor_controller.cursor_x - 1);
            self.cursor_controller.cursor_x -= 1;
        } else {
            let previous_row_content = self.editor_rows.get_editor_row(
                self.cursor_controller.cursor_y - 1
            );
            self.cursor_controller.cursor_x = previous_row_content.row_content.len();
            self.editor_rows.join_adjacent_rows(self.cursor_controller.cursor_y);
            self.cursor_controller.cursor_y -= 1;
        }
        if let Some(it) = self.syntax_highlight.as_ref() {
            it.update_syntax(self.cursor_controller.cursor_y, &mut self.editor_rows.row_contents);
        }
        self.dirty += 1;
    }

    fn insert_newline(&mut self) {
        let current_cursor_x = self.cursor_controller.cursor_x;
        let current_cursor_y = self.cursor_controller.cursor_y;

        if current_cursor_x == 0 {
            self.editor_rows.insert_row(current_cursor_y, String::new());
        } else {
            let current_row = self.editor_rows.get_editor_row_mut(current_cursor_y);
            let new_row_content = current_row.row_content[current_cursor_x..].to_string();
            current_row.row_content.truncate(current_cursor_x);
            EditorRows::render_row(current_row);
            self.editor_rows.insert_row(current_cursor_y + 1, new_row_content);

            if let Some(it) = self.syntax_highlight.as_ref() {
                it.update_syntax(
                    self.cursor_controller.cursor_y + 1,
                    &mut self.editor_rows.row_contents
                );
            }
        }
        self.cursor_controller.cursor_x = 0;
        self.cursor_controller.cursor_y += 1;
        self.dirty += 1;
    }

    fn find_callback(output: &mut Output, keyword: &str, key_code: KeyCode) {
        match key_code {
            KeyCode::Esc | KeyCode::Enter {} => {
                output.search_index.reset();
            }
            _ => {
                output.search_index.y_direction = None;
                output.search_index.x_direction = None;
                if let Some((idx, highlight)) = output.search_index.previous_highlight.take() {
                    output.editor_rows.get_editor_row_mut(idx).highlight = highlight;
                }
                match key_code {
                    KeyCode::Up => {
                        output.search_index.y_direction = Some(SearchDirection::Backward);
                    }
                    KeyCode::Down => {
                        output.search_index.y_direction = Some(SearchDirection::Forward);
                    }
                    KeyCode::Left => {
                        output.search_index.x_direction = Some(SearchDirection::Backward);
                    }
                    KeyCode::Right => {
                        output.search_index.x_direction = Some(SearchDirection::Forward);
                    }
                    _ => {}
                }
                for i in 0..output.editor_rows.number_of_rows() {
                    let row_idx = match output.search_index.y_direction {
                        Some(ref dir) => {
                            if matches!(dir, SearchDirection::Forward) {
                                output.search_index.y_index += i + 1;
                                output.search_index.y_index
                            } else {
                                let back = output.search_index.y_index.saturating_sub(i);
                                if back == 0 {
                                    break;
                                }
                                back - 1
                            }
                        }
                        None => {
                            if output.search_index.x_direction.is_none() {
                                output.search_index.y_index = i;
                            }
                            output.search_index.y_index
                        }
                    };
                    if row_idx > output.editor_rows.number_of_rows() - 1 {
                        // output.search_index.reset();
                        break;
                    }
                    let row = output.editor_rows.get_editor_row_mut(row_idx);
                    let idx = match output.search_index.x_direction {
                        Some(ref dir) => {
                            let idx = if matches!(dir, SearchDirection::Forward) {
                                let start = cmp::min(
                                    row.render.len(),
                                    output.search_index.x_index + 1
                                );

                                row.render[start..].find(keyword).map(|idx| idx + start)
                            } else {
                                row.render[..output.search_index.x_index].rfind(keyword)
                            };
                            if idx.is_none() {
                                break;
                            }
                            idx
                        }
                        None => { row.render.find(keyword) }
                    };
                    if let Some(idx) = idx {
                        output.search_index.previous_highlight = Some((
                            row_idx,
                            row.highlight.clone(),
                        ));
                        // 给keyword添加高亮
                        (idx..idx + keyword.len()).for_each(|i| {
                            row.highlight[i] = HighlightType::SearchMatch;
                        });
                        output.cursor_controller.cursor_x =
                            row.get_row_content_x(idx) + keyword.len();
                        output.cursor_controller.cursor_y = row_idx;
                        output.search_index.y_index = row_idx;
                        output.search_index.x_index = idx;
                        output.cursor_controller.row_offset = output.editor_rows.number_of_rows();
                        break;
                    }
                }
            }
        }
    }
    fn find(&mut self) -> io::Result<()> {
        // restore cursor position
        let cursor_controller = self.cursor_controller;
        let prompt = prompt!(
            self,
            "Search: {} (ESC / Arrows / Enter)",
            callback = Output::find_callback
        );
        if prompt.is_none() {
            self.cursor_controller = cursor_controller;
        }
        Ok(())
    }
    fn select_syntax(extension: &str) -> Option<Box<dyn SyntaxHighlight>> {
        let  extension_list: Vec<Box<(dyn SyntaxHighlight)>> = vec![Box::new(RustHighlight::new())];
        extension_list.into_iter().find(|it| it.extensions().contains(&extension))
    }
}

fn main() -> crossterm::Result<()> {
    let _clean_up = CleanUp;
    terminal::enable_raw_mode()?;
    let mut editor = Editor::new();
    while editor.run()? {}
    Ok(())
}