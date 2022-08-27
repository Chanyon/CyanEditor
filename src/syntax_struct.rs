use crossterm::{ queue, style::{ Color, SetForegroundColor } };
use crate::{ editor_row::{ Row, HighlightType }, EditorContents };

// 语法高亮
pub trait SyntaxHighlight {
    fn update_syntax(&self, at: usize, editor_rows: &mut Vec<Row>);
    fn syntax_color(&self, highlight: &HighlightType) -> Color;
    fn extensions(&self) -> &[&str];
    fn file_type(&self) -> &str;
    fn comment_start(&self) -> &str;
    fn multiline_comment(&self) -> Option<(&str, &str)>;
    fn color_row(&self, render: &str, highlight: &[HighlightType], out: &mut EditorContents) {
        let mut current_color = self.syntax_color(&HighlightType::Normal);
        render.char_indices().for_each(|(i, ch)| {
            let color = self.syntax_color(&highlight[i]);
            if current_color != color {
                current_color = color;
                let _ = queue!(out, SetForegroundColor(color));
            }
            out.push(ch);
        });
        let _ = queue!(out, SetForegroundColor(Color::Reset));
    }
    fn is_separator(&self, ch: char) -> bool {
        let separator = [
            ',',
            '.',
            '(',
            ')',
            '+',
            '-',
            '/',
            '*',
            '=',
            '~',
            '%',
            '<',
            '>',
            '"',
            '\'',
            ';',
            '&',
        ];
        ch.is_whitespace() || separator.contains(&ch)
    }
}

#[macro_export]
macro_rules! syntax_struct {
    (
        struct $Name:ident {
            extensions: $ext:expr,
            file_type: $type:expr,
            comment_start: $start:expr,
            keywords: { $([$color:expr; $($words:expr),*]),* },
            multiline_comment: $ml_comment:expr
        }
    ) => {
    struct $Name<'a>{
      extensions: &'a[&'a str],
      file_type: &'a str,
      comment_start: &'a str,
      multiline_comment: Option<(&'a str, &'a str)>,
    }

    impl $Name<'_>{
      fn new() -> Self {
        // $(
        //   let color = $color;
        //   let keywords = vec![$($words),*];
        // )*
        Self {
          extensions: $ext,
          file_type: $type,
          comment_start: $start,
          multiline_comment: $ml_comment,
        }
      }
    }

    impl SyntaxHighlight for $Name<'_> {
      fn update_syntax(&self, at: usize, editor_rows: &mut Vec<Row>) {
        let mut in_comment = at > 0 && editor_rows[at - 1].is_comment;
        let current_row = &mut editor_rows[at];
        current_row.highlight = Vec::with_capacity(current_row.render.len());
        let render = current_row.render.as_bytes();

        macro_rules! add {
          ($h:expr) => {
            current_row.highlight.push($h)
          };
        }

        let mut i = 0;
        let mut previous_separator = true;
        let mut in_string: Option<char> = None;
        let comment_start = self.comment_start().as_bytes();
        let render_len = render.len();

        while i < render_len {
          let ch = render[i] as char;

          let previous_highlight = if i > 0 {
            current_row.highlight[i - 1]
          }else {
            HighlightType::Normal
          };

          // comment
          if in_string.is_none() && !comment_start.is_empty() {
            let end = i + comment_start.len();
            if render[i..cmp::min(end,render_len)] == *comment_start {
              (i..render_len).for_each(|_| add!( HighlightType::Comment ));
              break;
            }
          }

          if let Some(val) = in_string {
            add!(if val == '"' { HighlightType::String } else { HighlightType::CharLiteral });
            if val == ch { in_string = None; }
            
            if ch == '\\' && i+1 < render_len {
              add!( if ch == '"' { HighlightType::String } else { HighlightType::CharLiteral });
              i += 2;
              continue;
            }

            i += 1;
            previous_separator = true;
            continue;
          }else {
            if (ch == '"' || ch == '\'') && !in_comment {
              in_string = Some(ch);
              add!(if ch == '"' { HighlightType::String } else { HighlightType::CharLiteral });
              i += 1;
              // previous_separator = false;
              continue;
            }
          }

          if ch.is_digit(10) && !in_comment
          && (previous_separator || matches!(previous_highlight,HighlightType::Number)) 
          || (ch == '.' && matches!(previous_highlight, HighlightType::Number)) {
            add!(HighlightType::Number);
            i += 1;
            previous_separator = false;
            continue;
          }

          // 关键字
          if previous_separator && !in_comment {
            $(
              $(
                  let end = i + $words.len();
                  let is_end_sep = render
                    .get(end)
                    .map(|ch| self.is_separator(*ch as char))
                    .unwrap_or(end == render_len);
                  if is_end_sep && render[i..end] == *($words.as_bytes()) {
                    (i..i + $words.len()).for_each(|_| add!(HighlightType::Other($color)));
                    i += $words.len();
                    previous_separator = false;
                    continue;
                  }
              )*
            )*
          }
          // /** */ml_comment
          if let Some(val) = $ml_comment {
            if in_string.is_none() {
              if in_comment {
                add!(HighlightType::MultilineComment);
                let end = i + val.1.len();
                if render[i..cmp::min(end, render_len)] == *(val.1.as_bytes()) {
                  (0..val.1.len().saturating_sub(1)).for_each(|_| add!(HighlightType::MultilineComment));
                  
                  i += val.1.len();
                  previous_separator = true;
                  in_comment = false;
                  continue;
                }else {
                  i += 1;
                  continue;
                }
              }else {
                let end = i + val.0.len();
                if render[i..cmp::min(end, render_len)] == *(val.0.as_bytes()) {
                  (i..end).for_each(|_| add!(HighlightType::MultilineComment));
                  i += val.0.len();
                  in_comment = true;
                  continue;
                } 
              }
            }
          }

          // comment
          if in_string.is_none() && !comment_start.is_empty() && !in_comment {
            let end = i + comment_start.len();
            if render[i..cmp::min(end, render_len)] == *(comment_start) {
              (i..render_len).for_each(|_| add!(HighlightType::Comment));
              break;
            }
          }

          add!(HighlightType::Normal);
          previous_separator = self.is_separator(ch);
          i += 1;
        }
        assert_eq!(current_row.render.len(), current_row.highlight.len());
        
        // /* ml_comment 之间全部注释*/
        let changed = current_row.is_comment != in_comment;
        current_row.is_comment = in_comment;
        if changed && (at + 1 < editor_rows.len()) {
          self.update_syntax(at+1, editor_rows);
        }
      }

      fn syntax_color(&self, highlight_type: &HighlightType) -> Color {
        match highlight_type {
          HighlightType::Normal => Color::Reset,
          HighlightType::Number => Color::Cyan,
          HighlightType::SearchMatch => Color::Blue,
          HighlightType::String => Color::Yellow,
          HighlightType::CharLiteral => Color::DarkYellow,
          HighlightType::Comment | HighlightType::MultilineComment => Color::DarkGrey,
          HighlightType::Other(color) => *color,
        }
      }

      fn extensions(&self) -> &[&str] {
        self.extensions
      }

      fn file_type(&self) -> &str {
        self.file_type
      }

      fn comment_start(&self) -> &str {
        self.comment_start
      }
      fn multiline_comment(&self) -> Option<(&str,&str)> {
        self.multiline_comment
      }
    }
    };
}