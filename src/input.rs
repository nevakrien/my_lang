use crate::parse::Loc;
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




pub struct MacroCall {
    pub depth:usize,

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


#[derive(Default)]
pub struct SourceContext{
    path_map:Mutex<HashMap<Arc<Path>,usize>>,
    pub files:boxcar::Vec<FileInfo>,
    pub macros:boxcar::Vec<MacroCall>,
}


impl SourceContext {
    pub fn new()->Self{
        Self::default()
    }
    pub fn get_file(&self,id:FileId)->Option<&FileInfo>{
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

    pub fn get_depth(&self,loc:Loc)->usize{
        match loc.src {
            Source::File(_) => 0,
            Source::Macro(id) => self.macros[id.0 as usize].depth,
        }
    }

    pub fn add_macro(&self,src:Loc,text:String)->MacroId {
        let depth = self.get_depth(src);
        let id = self.macros.push_with(|id|{
            MacroCall{
                depth,

                id:MacroId(id as u32),
                src,
                text,
                line_map:OnceCell::new(),
            }
        });

        MacroId(id as u32)
    }


    pub fn get_macro(&self,id:MacroId)->Option<&MacroCall>{
        self.macros.get(id.0 as usize)
    }
}



#[derive(Debug)]
pub struct MappedError<'a, E: Error> {
    pub inner: E,
    pub spans: Vec<MappedSpan<'a>>,
}

#[derive(Debug)]
pub struct MappedSpan<'a> {
    pub src: Source,
    pub line: LineNum,
    pub line_text: &'a str,
    pub col_start: usize,
    pub col_end: usize,
}

impl<'a, E: Error> fmt::Display for MappedError<'a, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.inner)?;
        let mut last_src: Option<Source> = None;
        for span in &self.spans {
            if last_src != Some(span.src) {
                writeln!(f, "\nIn {:?}:", span.src)?;
                last_src = Some(span.src);
            }

            writeln!(f, " --> line {}", span.line.0)?;
            writeln!(f, "  | {}", span.line_text)?;
            writeln!(
                f,
                "  | {}{}",
                " ".repeat(span.col_start),
                "^".repeat(span.col_end.saturating_sub(span.col_start).max(1))
            )?;
        }
        Ok(())
    }
}

impl<'a, E: Error > Error for MappedError<'a, E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.inner.source()
    }
}

impl<'a> SourceContext {
    pub fn add_context<E: Error>(&'a self, error: Located<E>) -> MappedError<'a, E> {
        let mut loc = error.loc;
        let mut spans = Vec::with_capacity(1);

        while let Source::Macro(id) = loc.src {
           spans.push(self.map_loc_to_span(loc));
           loc = self.get_macro(id).expect("macro does not exist").src; 
        }

        spans.push(self.map_loc_to_span(loc));

        MappedError {
            inner: error.value,
            spans,
        }
    }

    #[inline]
    pub fn combine_locs(&'a self,a:Loc,b:Loc)->Option<Loc>{
        if let Some(ans) = a.simple_combine(&b){
            return Some(ans)
        }
        self.combine_locs_slow(a,b)
    }

    pub fn combine_locs_slow(&'a self,mut a:Loc,mut b:Loc)->Option<Loc>{
        loop {
            match(a.src,b.src){
                (Source::File(_),Source::File(_))=>return a.simple_combine(&b),

                (Source::File(_), Source::Macro(m)) => {
                    b = self.get_macro(m)?.src;
                },
                (Source::Macro(m), Source::File(_)) => {
                    a = self.get_macro(m)?.src;
                },

                (Source::Macro(aid),Source::Macro(bid))=>{
                    let mut macro_a = self.get_macro(aid)?;
                    let mut macro_b = self.get_macro(bid)?;

                    //1. normalize heigt so both are on same depth
                    while macro_a.depth<macro_b.depth{
                        a = macro_a.src;
                        let Source::Macro(id) = a.src else{
                            panic!("bad depth numbers");
                        };
                        macro_a = self.get_macro(id)?;
                    }

                    while macro_b.depth<macro_a.depth{
                        b = macro_b.src;
                        let Source::Macro(id) = b.src else{
                            panic!("bad depth numbers");
                        };
                        macro_b = self.get_macro(id)?;
                    }

                    //2. bubele up untill both are in the same scope
                    loop {
                        if let Some(ans) = a.simple_combine(&b){
                            return Some(ans)
                        }

                        match a.src{
                            Source::Macro(id)=>{
                                a = self.get_macro(id)?.src;
                            },
                            Source::File(_)=>{
                                break;
                            }
                        };

                        match b.src{
                            Source::Macro(id)=>{
                                b = self.get_macro(id)?.src;
                            },
                            Source::File(_)=>{
                                break;
                            }
                        }
                    }
                },

            }
        }

    }

    fn map_loc_to_span(&'a self, loc: Loc) -> MappedSpan<'a> {
        let (map, text) = self.get_line_map_and_full_text(loc);
        let line = map.line_num(loc.start);
        let line_text = map.line_text(line, text);
        let start_of_line = map.line_start(line);
        let end_of_line = map.line_end(line);

        let col_start = loc.start - start_of_line;
        let col_end = loc.end.min(end_of_line) - start_of_line;

        MappedSpan {
            src: loc.src,
            line,
            line_text,
            col_start,
            col_end,
        }
    }

    pub fn get_line_map_and_full_text(&'a self,loc:Loc)->(&'a LineMap,&'a str){
        match loc.src {
            Source::File(id) => {
                let info = self.get_file(id).expect("file does not exist");
                (info.get_line_map(),info.text.get().unwrap().as_ref().unwrap())
            },
            Source::Macro(id) => {
                let info = self.get_macro(id).expect("macro does not exist");
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
    #[should_panic(expected = "missing closing quote in string")]
    fn mapped_error_missing_quote() {
        // 1️⃣ Prepare arenas
        let ctx = SourceContext::new();

        // 2️⃣ Add the *real* source text to the arena
        let path: Arc<Path> = Arc::from(Path::new("test_input.txt"));
        let file = ctx.get_for_path(path.clone());
        let src_text = r#"
"unterminated
second line
"#;
        file.text.set(Ok(src_text.to_string())).unwrap();

        // 3️⃣ Create the lexer using that *same* text slice
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
    #[should_panic(expected = "invalid number ending with")]
    fn mapped_error_weird_number_end() {
        let ctx = SourceContext::new();

        let path: Arc<Path> = Arc::from(Path::new("test_input.txt"));
        let file = ctx.get_for_path(path.clone());
        let src_text = r#"
 123x more stuff
other_line
"#;
        file.text.set(Ok(src_text.to_string())).unwrap();

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

    #[test]
    #[should_panic(expected = "demo numeric parse failure")]
    fn mapped_error_nested_macro_print() {
        let ctx = SourceContext::new();

        // --- outer file: defines and invokes a macro ---
        let path: Arc<Path> = Arc::from(Path::new("example.txt"));
        let file = ctx.get_for_path(path.clone());

        let text = "a + macro!(b + 1.2.3)\n";
        file.text.set(Ok(text.to_string())).unwrap();

        // locate "macro!(b + 1.2.3)" inside the outer file
        let outer_start = text.find("macro!(").unwrap();
        let outer_end = text.find(")\n").unwrap() + 1;
        let loc_outer = Loc {
            src: Source::File(file.id),
            start: outer_start,
            end: outer_end,
        };

        // --- simulate macro body text ---
        let macro_text = "b + 1.2.3"; // same string, just extracted body
        let macro_id = ctx.add_macro(loc_outer, macro_text.to_string());

        // locate bad token "1.2.3" inside the macro body
        let bad_start = macro_text.find("1.2.3").unwrap();
        let bad_end = bad_start + "1.2.3".len();
        let loc_inner = Loc {
            src: Source::Macro(macro_id),
            start: bad_start,
            end: bad_end,
        };

        // fake error to attach a location to
        #[derive(Debug, Error)]
        #[error("demo numeric parse failure")]
        struct FakeError;

        let located_err = Located {
            value: FakeError,
            loc: loc_inner,
        };

        // Build the mapped error (macro -> outer file)
        let mapped = ctx.add_context(located_err);

        // Panic to display the formatted mapping
        panic!("{}", mapped);
    }

    #[test]
    fn combine_locs_outer_scope() {
        // Case 1: both spans in the outer file scope
        let ctx = SourceContext::new();

        let path: Arc<Path> = Arc::from(Path::new("file.txt"));
        let file = ctx.get_for_path(path);
        let text = "aaa bbb ccc";
        file.text.set(Ok(text.to_string())).unwrap();

        let loc_a = Loc { src: Source::File(file.id), start: 0, end: 3 }; // "aaa"
        let loc_b = Loc { src: Source::File(file.id), start: 4, end: 7 }; // "bbb"

        let combined = ctx.combine_locs(loc_a, loc_b).unwrap();
        // expect span covering "aaa bbb"
        assert_eq!(combined.src, Source::File(file.id));
        assert_eq!(combined.start, 0);
        assert_eq!(combined.end, 7);
    }

    #[test]
    fn combine_locs_nested_macros_same_outer() {
        // Case 2: two different macros, both ultimately from the same outer call site
        let ctx = SourceContext::new();

        // outer file text with two macro calls
        let path: Arc<Path> = Arc::from(Path::new("file.txt"));
        let file = ctx.get_for_path(path);
        let text = "macro1!(a) + macro2!(b)";
        file.text.set(Ok(text.to_string())).unwrap();

        // locate both macro invocations
        let m1_start = text.find("macro1!(").unwrap();
        let m1_end = m1_start + "macro1!(a)".len();
        let m2_start = text.find("macro2!(").unwrap();
        let m2_end = m2_start + "macro2!(b)".len();

        let loc_m1_outer = Loc { src: Source::File(file.id), start: m1_start, end: m1_end };
        let loc_m2_outer = Loc { src: Source::File(file.id), start: m2_start, end: m2_end };

        // each macro expands from those sites
        let macro1 = ctx.add_macro(loc_m1_outer, "a + 1".to_string());
        let macro2 = ctx.add_macro(loc_m2_outer, "b + 2".to_string());

        // pick spans *inside* each macro body
        let loc_a = Loc { src: Source::Macro(macro1), start: 0, end: 1 }; // "a"
        let loc_b = Loc { src: Source::Macro(macro2), start: 0, end: 1 }; // "b"

        // combine — should bubble up and produce a combined outer span covering both call sites
        let combined = ctx.combine_locs(loc_a, loc_b).unwrap();
        assert_eq!(combined.src, Source::File(file.id));
        assert_eq!(combined.start, m1_start);
        assert_eq!(combined.end, m2_end);
    }

    #[test]
    fn combine_locs_within_nested_macro() {
        // Case 3: both spans inside the same macro (possibly nested)
        let ctx = SourceContext::new();

        // outer file has one macro call
        let path: Arc<Path> = Arc::from(Path::new("outer.txt"));
        let file = ctx.get_for_path(path);
        let outer_text = "macro!(inner!(x + y))";
        file.text.set(Ok(outer_text.to_string())).unwrap();

        let outer_start = outer_text.find("macro!(").unwrap();
        let outer_end = outer_text.find(')').unwrap() + 1;
        let loc_outer = Loc { src: Source::File(file.id), start: outer_start, end: outer_end };

        // outer macro expands to something with an inner macro call
        let macro1 = ctx.add_macro(loc_outer, "inner!(x + y)".to_string());
        let macro_text = "x + y";
        let inner_call_start = 7; // "inner!(" starts at byte 7
        let inner_call_end = inner_call_start + "inner!(x + y)".len();
        let loc_inner_call = Loc { src: Source::Macro(macro1), start: inner_call_start, end: inner_call_end };

        // inner macro expands to just "x + y"
        let macro2 = ctx.add_macro(loc_inner_call, macro_text.to_string());

        let loc_a = Loc { src: Source::Macro(macro2), start: 0, end: 1 }; // "x"
        let loc_b = Loc { src: Source::Macro(macro2), start: 4, end: 5 }; // "y"

        // combine — should succeed entirely within the inner macro
        let combined = ctx.combine_locs(loc_a, loc_b).unwrap();
        assert_eq!(combined.src, Source::Macro(macro2));
        assert_eq!(combined.start, 0);
        assert_eq!(combined.end, 5);
    }


}
