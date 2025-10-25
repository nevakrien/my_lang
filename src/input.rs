use crate::parse::Located;
use std::fmt;
use std::error::Error;
use std::io;
use thiserror::Error;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

#[repr(C)]
#[derive(Clone,Copy,PartialEq,Debug,Hash)]
pub struct Loc {
    pub src:Source,
    pub start:usize,
    pub end:usize,//exclusive
}

impl Loc {
    #[inline]
    pub fn simple_combine(&self,other:&Loc)->Option<Loc>{
        if self.src != other.src {
            return None
        }

        Some(Loc{
            src:self.src,
            start:self.start.min(other.start),
            end:self.end.max(other.end),
        })
    }
}

#[repr(C,u32)]
#[derive(Clone,Copy,PartialEq,Eq,Debug,Hash)]
pub enum Source{
    File(FileId) = 0,
    Macro(MacroId)=1,
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(
        size_of::<Source>() == 8,
        "we want a nice single reg"
    );
};

#[repr(C)]
#[derive(Clone,Copy,PartialEq,Eq,Debug,Hash)]
pub struct FileId(pub u32);

#[repr(C)]
#[derive(Clone,Copy,PartialEq,Eq,Debug,Hash)]
pub struct MacroId(pub u32);

#[derive(Debug, Error)]
#[error("failed to read '{path}' because of {inner}")]
pub struct FileError {
    pub path: Arc<Path>,
    #[source]
    pub inner: io::Error,
}

pub struct FileInfo {
    pub id:FileId,
    pub path:Arc<Path>,
    pub text:OnceCell<Result<String,FileError>>,
    pub line_map:OnceCell<LineMap>,
}

impl FileInfo{
    pub fn get_line_map(&self)->&LineMap{
        let text = self.text.wait().as_ref().unwrap();
        self.line_map.get_or_init(||{LineMap::new(text)})
    }
}

impl FileInfo {
    #[inline]
    pub fn get_text(&self,loc:Loc)->&str{
        assert!(
            match loc.src {
                Source::File(fid) => fid == self.id,
                _ => false,
            },
            "mismatched source: expected {:?}, got {:?}",
            self.id,
            loc.src
        );

        //if this is called loc exists so file has to make sense
        let s = self.text.get().unwrap().as_ref().unwrap().as_str();
        &s[loc.start..loc.end]

    }
}

pub struct FileArena{
    path_map:Mutex<HashMap<Arc<Path>,usize>>,
    pub files:boxcar::Vec<FileInfo>
}

impl FileArena {
    pub fn get(&self,id:FileId)->Option<&FileInfo>{
        self.files.get(id.0 as usize)
    }
    pub fn load_file(&self,path:Arc<Path>)->&FileInfo{
        let file = self.get_for_path(path.clone());
        file.text.get_or_init(||{
            std::fs::read_to_string(&path).map_err(|inner|{
                FileError{
                    inner,
                    path
                }
            })
        });
        file
    }
    pub fn get_for_path(&self,path:Arc<Path>)->&FileInfo{
        use std::collections::hash_map::Entry;

        let mut map = self.path_map.lock().unwrap();
        let file_id = match map.entry(path) {
            Entry::Occupied(o) => return &self.files[*o.get()], 
            Entry::Vacant(v) => {
                self.files.push_with(|id|{
                    FileInfo{
                        id:FileId(id as u32),
                        path:v.key().clone(),
                        text:OnceCell::new(),
                        line_map:OnceCell::new(),
                    }

                })
            },
        };

        //drop the mutex we are done with it
        std::mem::drop(map);
        self.files.get(file_id).unwrap()
    }
}

pub struct MacroCall {
    pub id:MacroId,
    pub src:Loc,
    pub text:String,//not sure yet if tokens or not
    pub line_map:OnceCell<LineMap>,
}

impl MacroCall{
    pub fn get_line_map(&self)->&LineMap{
        self.line_map.get_or_init(||{LineMap::new(&self.text)})
    }
}


pub struct MacroArena(boxcar::Vec<MacroCall>);

impl MacroArena {
    pub fn add(&self,src:Loc,text:String)->MacroId {
        let id = self.0.push_with(|id|{
            MacroCall{
                id:MacroId(id as u32),
                src,
                text,
                line_map:OnceCell::new(),
            }
        });

        MacroId(id as u32)
    }


    pub fn get(&self,id:MacroId)->Option<&MacroCall>{
        self.0.get(id.0 as usize)
    }
}

#[derive(Debug)]
pub struct MappedError<'a, E: Error> {
    pub inner: E,
    pub line: LineNum,
    pub line_text: &'a str,
    pub col_start: usize,
    pub col_end: usize,
}

impl<'a, E: Error> MappedError<'a, E> {
    #[inline]
    pub fn new(inner: E, map: &'a LineMap, text: &'a str, loc: Loc) -> Self {
        let line = map.line_num(loc.start);
        let line_text = map.line_text(line, text);


        let start_of_line = map.line_start(line);
        let end_of_line = map.line_end(line);

        let col_start = loc.start - start_of_line;
        let col_end = loc.end.min(end_of_line) - start_of_line;

        Self { inner, line, line_text, col_start, col_end }
    }
}

impl<'a, E: Error> fmt::Display for MappedError<'a, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.inner)?;
        writeln!(f, " --> line {}", self.line.0)?;
        writeln!(f, "  | {}", self.line_text)?;
        writeln!(
            f,
            "  | {}{}",
            " ".repeat(self.col_start),
            "^".repeat(self.col_end.saturating_sub(self.col_start).max(1))
        )
    }
}

impl<'a, E: Error + 'static> Error for MappedError<'a, E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.inner)
    }
}

#[derive(Clone,Copy)]
pub struct Context<'a>{
    pub files:&'a FileArena,
    pub macros:&'a MacroArena,
}

impl<'a> Context<'a> {
    pub fn get_text(&self,loc:Loc)->&'a str{
        match loc.src {
            Source::File(id) => {
                let info = self.files.get(id).expect("file does not exist");
                let text = info.text.get().unwrap().as_ref().unwrap();
                &text[loc.start..loc.end]
            },
            Source::Macro(id) => {
                let info = self.macros.get(id).expect("macro does not exist");
                let text = &info.text;
                &text[loc.start..loc.end]
            }, 
        }
    }

    pub fn add_context<E:Error>(&self,error:Located<E>)->MappedError<'a, E>{
        let (map,text) = self.get_line_map_and_full_text(error.loc);
        MappedError::new(error.value,map,text,error.loc)
    }

    pub fn get_line_map_and_full_text(&self,loc:Loc)->(&'a LineMap,&'a str){
        match loc.src {
            Source::File(id) => {
                let info = self.files.get(id).expect("file does not exist");
                (info.get_line_map(),info.text.get().unwrap().as_ref().unwrap())
            },
            Source::Macro(id) => {
                let info = self.macros.get(id).expect("macro does not exist");
                (info.get_line_map(),info.text.as_ref())
            }, 
        }

    }
}

/// 1 indexed line number
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
pub struct LineNum(pub usize);
pub struct LineMap(Vec<isize>);

impl LineMap {
    pub fn new(text:&str)->Self{
        let mut v = vec![-1];
        for (i,c) in text.char_indices(){
            if c=='\n'{
                v.push(i as isize)
            }

        }

        v.push(text.len()as isize);

        Self(v)
    }

    pub fn line_num(&self,byte:usize)->LineNum{
        match self.0.binary_search(&(byte as isize)){
            Ok(idx)=>LineNum(idx),
            Err(idx)=>LineNum(idx)
        }
    }

    pub fn line_start(&self,line:LineNum)->usize{
        (self.0[line.0-1]+1) as usize
    }

    pub fn line_end(&self,line:LineNum)->usize{
        self.0[line.0] as usize
    }

    pub fn line_text<'a>(&self,line:LineNum,text:&'a str)->&'a str{
        let start = self.line_start(line);
        let end = self.line_end(line);
        &text[start..end]
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_line_text(map: &LineMap, text: &str, line: usize, expected: &str) {
        let ln = LineNum(line);
        let slice = map.line_text(ln, text);
        assert_eq!(slice, expected, "line_text({line}) mismatch");
    }

    #[test]
    fn ascii_without_trailing_newline() {
        let text = "foo\nbar\nbaz";
        // line starts at [0, 4, 8]
        let map = LineMap::new(text);

        assert_eq!(map.line_start(LineNum(1)), 0);
        assert_eq!(map.line_end(LineNum(1)), 3);
        assert_eq!(map.line_start(LineNum(2)), 4);
        assert_eq!(map.line_end(LineNum(2)), 7);
        assert_eq!(map.line_start(LineNum(3)), 8);
        assert_eq!(map.line_end(LineNum(3)), 11);

        check_line_text(&map, text, 1, "foo");
        check_line_text(&map, text, 2, "bar");
        check_line_text(&map, text, 3, "baz");

        // check line number lookup for various bytes
        assert_eq!(map.line_num(0), LineNum(1)); // 'f'
        assert_eq!(map.line_num(3), LineNum(1)); // '\n' belongs to previous
        assert_eq!(map.line_num(4), LineNum(2)); // 'b'
        assert_eq!(map.line_num(8), LineNum(3)); // 'b' of baz
        assert_eq!(map.line_num(text.len() - 1), LineNum(3)); // 'z'
        assert_eq!(map.line_num(text.len()), LineNum(3)); // just after text
    }

    #[test]
    fn ascii_with_trailing_newline() {
        let text = "foo\nbar\nbaz\n";
        // last line is empty but still counted
        let map = LineMap::new(text);

        assert_eq!(map.line_start(LineNum(1)), 0);
        assert_eq!(map.line_start(LineNum(2)), 4);
        assert_eq!(map.line_start(LineNum(3)), 8);
        assert_eq!(map.line_start(LineNum(4)), 12);

        check_line_text(&map, text, 1, "foo");
        check_line_text(&map, text, 2, "bar");
        check_line_text(&map, text, 3, "baz");
        check_line_text(&map, text, 4, ""); // empty trailing line

        // newline at end of line 3
        assert_eq!(map.line_num(11), LineNum(3)); // '\n' after baz
        assert_eq!(map.line_num(12), LineNum(4)); // new empty line
        assert_eq!(map.line_num(text.len()), LineNum(4)); // after last byte
    }

    #[test]
    fn unicode_mixed_lines() {
        let text = "αβγ\nπρσ\n終わり";
        // bytes: αβγ = 6 bytes, πρσ = 6, 終わり = 9
        let map = LineMap::new(text);

        check_line_text(&map, text, 1, "αβγ");
        check_line_text(&map, text, 2, "πρσ");
        check_line_text(&map, text, 3, "終わり");

        assert_eq!(map.line_num(0), LineNum(1));   // α
        assert_eq!(map.line_num(6), LineNum(1));   // '\n'
        assert_eq!(map.line_num(7), LineNum(2));   // π
        assert_eq!(map.line_num(13), LineNum(2));  // '\n'
        assert_eq!(map.line_num(14), LineNum(3));  // 終
        assert_eq!(map.line_num(text.len() - 1), LineNum(3)); // last byte
    }

    #[test]
    fn empty_and_single_line() {
        let empty = "";
        let map = LineMap::new(empty);
        check_line_text(&map, empty, 1, "");
        assert_eq!(map.line_num(0), LineNum(1));

        let single = "one line only";
        let map = LineMap::new(single);
        check_line_text(&map, single, 1, "one line only");
        assert_eq!(map.line_num(0), LineNum(1));
        assert_eq!(map.line_num(single.len()), LineNum(1));
    }

    #[test]
    fn basic(){
        let text = r#""unterminated"#;
        let map = LineMap::new(text);
        check_line_text(&map,text,1,text);
    }
}
#[cfg(test)]
mod mapped_error_format_tests {
    use super::*;
    use crate::parse::BasicLexer;
    use crate::input::Source;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    #[should_panic]
    fn mapped_error_missing_quote() {
        // 1️⃣ Prepare arenas
        let files = FileArena {
            path_map: Mutex::new(HashMap::new()),
            files: boxcar::Vec::new(),
        };
        let macros = MacroArena(boxcar::Vec::new());

        // 2️⃣ Add the *real* source text to the arena
        let path: Arc<Path> = Arc::from(Path::new("test_input.txt"));
        let file = files.get_for_path(path.clone());
        let src_text = r#"
"unterminated
second line
"#;
        file.text.set(Ok(src_text.to_string())).unwrap();

        // 3️⃣ Create the lexer using that *same* text slice
        let ctx = Context { files: &files, macros: &macros };
        let src = Source::File(file.id);
        let mut lex = BasicLexer::new(src_text, src);

        // 4️⃣ Force the lexer to hit MissingStringClose
        match lex.next() {
            Err(loc_err) => {
                let mapped = ctx.add_context(loc_err);
                panic!("{}", mapped);
            }
            Ok(_) => panic!("expected MissingStringClose"),
        }
    }

    #[test]
    #[should_panic]
    fn mapped_error_weird_number_end() {
        let files = FileArena {
            path_map: Mutex::new(HashMap::new()),
            files: boxcar::Vec::new(),
        };
        let macros = MacroArena(boxcar::Vec::new());

        let path: Arc<Path> = Arc::from(Path::new("test_input.txt"));
        let file = files.get_for_path(path.clone());
        let src_text = r#"
 123x
other_line
"#;
        file.text.set(Ok(src_text.to_string())).unwrap();

        let ctx = Context { files: &files, macros: &macros };
        let src = Source::File(file.id);
        let mut lex = BasicLexer::new(src_text, src);

        // Force WeirdNumberEnd
        match lex.next() {
            Err(loc_err) => {
                let mapped = ctx.add_context(loc_err);
                panic!("{}", mapped);
            }
            Ok(_) => panic!("expected WeirdNumberEnd"),
        }
    }
}
