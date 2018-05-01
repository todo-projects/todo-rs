use std::fs;
use std::path::Path;
use std::io::{Read, Write};
use failure::Error;
use todo::error::TodoError;
use todo::command::Command;
use todo::command::store::Create as CanCreate;
use todo::issue::{Content, Issue};
use todo::tools::map_str;

#[derive(Clone, Debug, Default)]
pub struct Create {
    content: String,
    pub format: Option<String>,
    pub dir: Option<String>,
    pub ext: Option<String>,
    pub path: Option<String>,
    pub id_generator: Option<SequenceGenerator>,
}

impl Command for Create {
    fn set_param(&mut self, key: &str, value: String) -> Result<(), TodoError> {
        match key.to_lowercase().as_str() {
            "ext" => self.ext = Some(value),
            "path" => self.path = Some(value),
            _ => return Err(TodoError::UnknownCommandParam { param: key.to_string() }),
        }
        Ok(())
    }

    fn exec(&mut self) {
        if let Some(ref str_path) = self.path {
            let path = Path::new(str_path);

            fs::File::open(path).expect_err(&format!("File {} already exists", str_path));
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).expect(&format!("Can't create dir: {:?}", dir));
            }
            let mut file = fs::File::create(path)
                .expect(&format!("Creation error with path: {}", str_path));
            if !self.content.is_empty() {
                file.write_all(self.content.as_bytes())
                    .expect(&format!("Error content write to file: {}", str_path));
            }

            println!("{}", str_path);
        }
    }
}

impl CanCreate for Create {
    fn init_from<T: Content>(&mut self, issue: &Issue<T>) {
        let mut format = map_str(&self.format, String::as_str).to_string();

        let mut id = issue.get_id().map(|id| id.clone()).unwrap_or_default();

        if let Some(ref generator) = self.id_generator {
            let id_found = format.find(&issue.id_attr_key)
                .and_then(|pos| format.key_replaceable_pos(pos, issue.id_attr_key.len()))
                .is_some();
            if id_found && issue.get_id().is_none() {
                id = generator.next().expect("Generate next id fail");
            }
        }

        format.key_replace(&issue.id_attr_key, id.as_str());
        format.key_replace("ext", map_str(&self.ext,|ext| ext.as_str()));
        for key in issue.attrs.keys.iter() {
            let key = key.as_str();
            if key != issue.id_attr_key {
                let value = issue.attrs.attr_value(key)
                    .map(|s| s.as_str())
                    .unwrap_or_default();
                if !format.key_replace(key, value) {
                    self.content += &format!("#[{}: {}]\n", key, value);
                }
            }
        }

        if let Some(ref dir) = self.dir {
            self.path = Some(format!("{}/{}", dir, format));
        }
    }
}

trait Format {
    fn find_from_pos(&self, pos: usize, needle: &str) -> Option<usize>;
    fn find_byte(&self, start: usize, needle: u8) -> Option<usize>;
    fn rfind_byte(&self, end: usize, needle: u8) -> Option<usize>;
    fn key_replaceable_pos(&self, key_pos: usize, key_len: usize) -> Option<(usize, usize)>;
    fn key_replace(&mut self, key: &str, value: &str) -> bool;
}

impl Format for String {
    fn find_from_pos(&self, pos: usize, needle: &str) -> Option<usize> {
        let end = self.len() - needle.len() + 1;
        if pos < end {
            for i in pos..end {
                for j in 0..needle.len() {
                    if self.as_bytes()[i + j] != needle.as_bytes()[j] {
                        break;
                    }
                    if j == needle.len() - 1 {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    fn find_byte(&self, start: usize, needle: u8) -> Option<usize> {
        let source = self.as_bytes();
        for i in start..source.len() {
            if source[i] == needle {
                return Some(i);
            }
        }
        None
    }

    fn rfind_byte(&self, end: usize, needle: u8) -> Option<usize> {
        let source = self.as_bytes();
        for i in (0..end).rev() {
            if source[i] == needle {
                return Some(i);
            }
        }
        None
    }

    fn key_replaceable_pos(&self, key_pos: usize, key_len: usize) -> Option<(usize, usize)> {
        let (mut start, mut end) = (0, 0);
        let mut found = false;

        let index = key_pos;
        if index > 0 && index + key_len < self.len() {
            if self.as_bytes()[index - 1] == b'{' {
                start = index - 1;
                found = true;
            } else if index > 1 && self.as_bytes()[index - 1] == b':' {
                if let Some(start_index) = self.rfind_byte(index - 1, b'{') {
                    start = start_index;
                    found = true;
                }
            }
        }

        if found {
            found = false;
            let index = index + key_len - 1;

            if self.as_bytes()[index + 1] == b'}' {
                end = index + 1;
                found = true;
            } else if index + 2 < self.as_bytes().len() && self.as_bytes()[index + 1] == b':' {
                if let Some(end_index) = self.find_byte(index + 2, b'}') {
                    end = end_index;
                    found = true;
                }
            }
        }

        if found {
            Some((start, end))
        } else {
            None
        }
    }

    fn key_replace(&mut self, key: &str, value: &str) -> bool {
        let mut replaced = false;
        let mut find_pos = 0;

        while let Some(index) = self.find_from_pos(find_pos, key) {
            find_pos = index + 1;

            if let Some((start, end)) = self.key_replaceable_pos(index, key.len()) {
                let before = if start + 1 < index - 1 {
                    String::from_utf8_lossy(&self.as_bytes()[(start + 1)..(index - 1)]).to_string()
                } else {
                    "".to_string()
                };

                let after = if index + key.len() + 1 < end {
                    String::from_utf8_lossy(&self.as_bytes()[(index + key.len() + 1)..end]).to_string()
                } else {
                    "".to_string()
                };

                let head = String::from_utf8_lossy(&self.as_bytes()[..start]).to_string();
                let tail = String::from_utf8_lossy(&self.as_bytes()[(end + 1)..]).to_string();

                let body = if !value.is_empty() {
                    format!("{}{}{}", before, value, after)
                } else {
                    "".to_string()
                };

                find_pos = head.len() + body.len() + tail.len();
                *self = format!("{}{}{}", head, body, tail);
                replaced = true;
            }
        }
        replaced
    }
}

#[derive(Clone, Debug, Default)]
pub struct SequenceGenerator {
    pub required: bool,
    pub file: Option<String>,
}

impl SequenceGenerator {
    pub fn next(&self) -> Result<String, Error> {
        match self.file {
            Some(ref path) => {
                let id = {
                    let mut contents = String::new();
                    let open = fs::File::open(path);
                    match open {
                        Ok(mut file) => {
                            file.read_to_string(&mut contents)?;
                            contents.trim().to_string()
                        },
                        Err(err) => return if self.required {
                            Err(err.into())
                        } else {
                            Ok("".to_string())
                        },
                    }
                };
                let new_id = format!("{}", id.parse::<u64>()? + 1);

                let mut file = fs::File::create(path)?;
                file.write_all(new_id.as_bytes())?;

                Ok(id)
            },
            None if self.required => Err(TodoError::FileNotSpecified.into()),
            _ => Ok("".to_string())
        }
    }
}