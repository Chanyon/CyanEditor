#[macro_export]
// 处理用户输入
macro_rules! prompt {
    ($output:expr, $args:tt) => {
      // 增量搜索
      {prompt!($output, $args, callback = |&_, _, _| {})}
    };
    ($output:expr, $args:tt, callback = $callback:expr) => {
        {
          let output:&mut Output = $output;
          let mut input = String::with_capacity(32);
          loop {
            output.status_message.set_message(format!($args,input));
            output.refresh_screen()?;
            let key_code = Reader.read_key()?;
            match key_code {
              KeyEvent {
                code: KeyCode::Enter,
                modifiers: event::KeyModifiers::NONE,
              } => {
                if !input.is_empty() {
                  output.status_message.set_message(String::new());
                  $callback(output, &input, KeyCode::Enter);
                  break;
                }
              },
              KeyEvent {
                code: KeyCode::Esc,
                modifiers: event::KeyModifiers::NONE,
              } => {
                  output.status_message.set_message(String::new());
                  input.clear();
                  $callback(output, &input, KeyCode::Esc);
                  break;
              },
              KeyEvent {
                code: KeyCode::Backspace | KeyCode::Delete,
                modifiers: event::KeyModifiers::NONE,
              } => {
                  input.pop();
              },
              KeyEvent {
                code: key @ (KeyCode::Char(..) | KeyCode::Tab),
                modifiers: event::KeyModifiers::NONE | event::KeyModifiers::SHIFT,
              } => {
                input.push(match key {
                  KeyCode::Tab => '\t',
                  KeyCode::Char(ch) => ch,
                  _ => unreachable!(),
                });
              },
              _ => { },
            }
            $callback(output, &input, key_code.code);
          }
          if input.is_empty() { None } else { Some(input) }
        }
    };
}