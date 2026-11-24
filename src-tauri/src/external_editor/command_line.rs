//! What starting a configured program comes to: the program and the list of arguments it is
//! given. Nothing here goes through a shell, so no character in a file's name or in the
//! configured arguments is ever read as a command: the arguments are split the way a shell
//! would split them, and the file's path goes in afterwards, whole.

use crate::error::{AppError, AppResult};

/// Stands for the file's path in a program's arguments.
pub const FILE_PLACEHOLDER: &str = "{file}";

/// A program to start and what it is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub arguments: Vec<String>,
}

/// The program named by `command`, taken whole so a path with spaces needs no quotes, with
/// `arguments` split and `file` put where they say. Arguments that do not name the file get
/// it as their last.
pub fn invocation(command: &str, arguments: &str, file: &str) -> AppResult<Invocation> {
    let program = command.trim();
    if program.is_empty() {
        return Err(AppError::invalid("The program has no command to run"));
    }
    let mut names_file = false;
    let mut arguments: Vec<String> = split_arguments(arguments)?
        .into_iter()
        .map(|argument| {
            if argument.contains(FILE_PLACEHOLDER) {
                names_file = true;
                argument.replace(FILE_PLACEHOLDER, file)
            } else {
                argument
            }
        })
        .collect();
    if !names_file {
        arguments.push(file.to_string());
    }
    Ok(Invocation {
        program: program.to_string(),
        arguments,
    })
}

/// Splits arguments at white space. Double quotes keep white space in one argument and take
/// `\"` and `\\` for a quote and a backslash; single quotes keep everything as written. A
/// backslash outside quotes is an ordinary character, as in a Windows path.
pub fn split_arguments(line: &str) -> AppResult<Vec<String>> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    // Set once the argument has begun, which an empty pair of quotes does without a character.
    let mut started = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' => {
                started = true;
                loop {
                    match characters.next() {
                        Some('"') => break,
                        Some('\\') if matches!(characters.peek(), Some('"' | '\\')) => {
                            current.extend(characters.next());
                        }
                        Some(quoted) => current.push(quoted),
                        None => return Err(unclosed('"')),
                    }
                }
            }
            '\'' => {
                started = true;
                loop {
                    match characters.next() {
                        Some('\'') => break,
                        Some(quoted) => current.push(quoted),
                        None => return Err(unclosed('\'')),
                    }
                }
            }
            character if character.is_whitespace() => {
                if started {
                    arguments.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            character => {
                started = true;
                current.push(character);
            }
        }
    }
    if started {
        arguments.push(current);
    }
    Ok(arguments)
}

fn unclosed(quote: char) -> AppError {
    AppError::invalid(format!("The arguments have a {quote} that is never closed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(line: &str) -> Vec<String> {
        split_arguments(line).unwrap()
    }

    #[test]
    fn arguments_split_at_white_space() {
        assert_eq!(split(""), Vec::<String>::new());
        assert_eq!(split("   \t "), Vec::<String>::new());
        assert_eq!(split("--wait"), ["--wait"]);
        assert_eq!(
            split("  -n   --line 5\t{file} "),
            ["-n", "--line", "5", "{file}"]
        );
    }

    #[test]
    fn quotes_keep_white_space_and_can_be_empty() {
        assert_eq!(
            split(r#"--title "My file" -x"#),
            ["--title", "My file", "-x"]
        );
        assert_eq!(split("--title 'My  file'"), ["--title", "My  file"]);
        assert_eq!(split(r#"a"b c"d"#), ["ab cd"]);
        assert_eq!(split(r#""" x ''"#), ["", "x", ""]);
        assert_eq!(split(r#""say \"hi\" \\ there""#), [r#"say "hi" \ there"#]);
        assert_eq!(split(r#"'it''s' "it's""#), ["its", "it's"]);
        // Only quotes and backslashes are escaped inside double quotes.
        assert_eq!(split(r#""C:\tools\new""#), [r"C:\tools\new"]);
    }

    #[test]
    fn backslashes_outside_quotes_are_kept() {
        assert_eq!(split(r"/select,C:\Users\me"), [r"/select,C:\Users\me"]);
        assert_eq!(split(r"a\ b"), [r"a\", "b"]);
    }

    #[test]
    fn quotes_left_open_are_refused() {
        assert!(split_arguments(r#"--title "My file"#).is_err());
        assert!(split_arguments("'half").is_err());
    }

    #[test]
    fn the_file_goes_where_the_arguments_say_as_one_argument() {
        let file = "/cache/editing/1/My report (final).xlsx";
        assert_eq!(
            invocation("code", "--wait {file}", file).unwrap(),
            Invocation {
                program: "code".to_string(),
                arguments: vec!["--wait".to_string(), file.to_string()],
            }
        );
        assert_eq!(
            invocation("tool", "--input={file} --mode 'read write'", file)
                .unwrap()
                .arguments,
            [
                format!("--input={file}"),
                "--mode".to_string(),
                "read write".to_string()
            ]
        );
        assert_eq!(
            invocation("tool", r#""{file}" {file}.bak"#, file)
                .unwrap()
                .arguments,
            [file.to_string(), format!("{file}.bak")]
        );
    }

    #[test]
    fn the_file_is_added_when_the_arguments_leave_it_out() {
        assert_eq!(
            invocation("gimp", "", "/tmp/a b.png").unwrap().arguments,
            ["/tmp/a b.png"]
        );
        assert_eq!(
            invocation("gimp", "--no-splash", "/tmp/a.png")
                .unwrap()
                .arguments,
            ["--no-splash", "/tmp/a.png"]
        );
    }

    #[test]
    fn nothing_in_a_file_name_becomes_an_argument_or_a_command() {
        let hostile = "/cache/editing/1/a; rm -rf ~ $(reboot) `id` \"q\" 'q' {file} && x.txt";
        let started = invocation("editor", "--open {file}", hostile).unwrap();
        assert_eq!(started.arguments, ["--open", hostile]);
        let started = invocation("editor", "", hostile).unwrap();
        assert_eq!(started.arguments, [hostile]);
    }

    #[test]
    fn the_program_is_taken_whole() {
        let started = invocation(
            r"  C:\Program Files\Notepad++\notepad++.exe ",
            "-multiInst",
            r"C:\cache\a.txt",
        )
        .unwrap();
        assert_eq!(started.program, r"C:\Program Files\Notepad++\notepad++.exe");
        assert!(invocation("  ", "{file}", "/tmp/a").is_err());
    }
}
