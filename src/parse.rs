use crate::types::GVarID;
use crate::input::SourceContext;
use crate::types::VarID;
use std::rc::Rc;
use std::sync::Arc;
use std::hash::Hash;
use thiserror::Error;
use std::collections::HashMap;
// use std::rc::Rc;
use crate::input::Source;
use std::ops::{Deref, DerefMut};

#[repr(C)]
#[derive(Clone,Copy,PartialEq,Debug,Hash)]
pub struct Loc {
    pub src:Source,
    pub start:usize,
    pub end:usize,//exclusive
}

impl Loc {
    #[inline]
    pub fn simple_combine(self,other:&Loc)->Option<Loc>{
        if self.src != other.src {
            return None
        }

        Some(Loc{
            src:self.src,
            start:self.start.min(other.start),
            end:self.end.max(other.end),
        })
    }

    pub fn with<T>(self,value:T)->Located<T>{
        Located{
            value,
            loc:self
        }
    }
}


#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub struct Located<T> {
    pub value: T,
    pub loc: Loc,
}

impl<T> Located<T> {
    #[inline]
    pub fn new(value: T, loc: Loc) -> Self {
        Self { value, loc }
    }

    pub fn into_inner(self)->T{
    	self.value
    }

    pub fn map_owned<U>(self,f:impl Fn(T)->U)->Located<U>{
    	Located{
    		value:f(self.value),
    		loc:self.loc
    	}
    }

    pub fn with<U>(&self,value:U)->Located<U>{
    	Located{
    		value,
    		loc:self.loc.clone()
    	}
    }

    pub fn fixtype<U:From<T>>(self)->Located<U>{
    	Located{
    		value: self.value.into(),
    		loc:self.loc
    	}
    }
}

impl<T> Deref for Located<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Located<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}




#[derive(Debug, Error,Clone)]
pub enum LexError {
    #[error("invalid number ending with '{0}'")]
    WeirdNumberEnd(char),
	// UnKnowenEscape(char),

    #[error("missing closing quote in string")]
    MissingStringClose,
}



type LexRes<T> = Result<T,Located<LexError>>;

#[derive(Debug, PartialEq, Hash)]
pub enum Token<'a>{
	Name(&'a str),
	Num(u64),
	Str(String),
}

#[derive(Debug,Clone,Copy)]
pub struct BasicLexer<'a>{
	cur_str:&'a str,
	cur_start:usize,
	src:Source
}

fn is_op_like(c:char)->bool{
	 
	!(c.is_alphanumeric() || c=='_' || c.is_whitespace()||c=='"')
}

impl<'a> BasicLexer<'a>{
	pub fn new(cur_str:&'a str,src:Source)->Self{
		Self{
			cur_str,
			cur_start:0,
			src,
		}
	}

	pub fn new_at(cur_str:&'a str,src:Source,cur_start:usize)->Self{
		Self{
			cur_str,
			cur_start,
			src,
		}
	}

	fn skip_whitespace(&mut self){
		for c in self.cur_str.chars(){
			if !c.is_whitespace(){
				return;
			}

			let size = c.len_utf8();
			self.cur_start+=size;
			self.cur_str=&self.cur_str[size..];
		}
	}

	fn skip_comments(&mut self){
		loop{
			self.skip_whitespace();
			if self.cur_str.as_bytes().get(..2) != Some(b"//"){
				return;
			}
			self.yeild_next(2);
			for c in self.cur_str.chars(){
				if c=='\n'{
					break;
				}

				let size = c.len_utf8();
				self.cur_start+=size;
				self.cur_str=&self.cur_str[size..];
			}
		}
	}

	fn yeild_next(&mut self,size:usize)->Located<&'a str>{
		let value = &self.cur_str[..size];
		let end = self.cur_start+size;

		let loc = Loc{
			src:self.src,
			start:self.cur_start,
			end,
		};

		let ans = Located::new(value,loc);

		if end-self.cur_start < self.cur_str.len(){
			self.cur_str=&self.cur_str[size..];
		}else{
			self.cur_str="";
		}
		self.cur_start=end;
		ans
	}

	fn parse_name(&mut self)->Located<Token<'a>>{
		let mut size = 0usize;
		for c in self.cur_str.chars(){
			if !(c.is_alphanumeric() || c=='_') {
				break;
			}

			size+=c.len_utf8();
		}

		self.yeild_next(size).map_owned(Token::Name)
	}

	fn parse_number(&mut self)->LexRes<Located<Token<'a>>>{
		let mut size = 0usize;
		for c in self.cur_str.chars(){
			if c.is_whitespace() {
				break;
			}

			if !(c.is_numeric() || c=='_') {
				if !c.is_alphabetic(){
					break;//we explictly allow 2+3 style parses
				}
				size+=c.len_utf8();
				let tok = self.yeild_next(size);
				let err = tok.map_owned(|_|{LexError::WeirdNumberEnd(c)});
				return Err(err);
			}

			size+=c.len_utf8();
		}

		let tok = self.yeild_next(size);
		let mut num = 0u64;
		for c in tok.as_bytes().iter() {
			if *c==b'_'{
				continue;
			}

			num=10*num+((c-b'0') as u64);

		}
		Ok(tok.with(Token::Num(num)))
	}

	fn parse_string(&mut self)->LexRes<Located<Token<'a>>>{
		let mut size = 1;

		let mut s = String::new();
		let mut skip = false;

		for c in self.cur_str[1..].chars(){
			size+=c.len_utf8();
			
			if c == '\n'{
				break
			}

			if skip {
				skip = false;
				let resolved = match c {
					'n'  => '\n',
					'r'  => '\r',
					't'  => '\t',
					'\\' => '\\',
					'"'  => '"',
					'\'' => '\'',
					'0'  => '\0',
					// 'v'  => '\x0B',
					// 'f'  => '\x0C',

					//there is a good argument for erroring here
					//the issue is we dont wana compltly destroy the parse...
					//which implies multi error and thats really tricky because heap...
					//so for now we dont bother
					_    => c,//return Err(self.yeild_next(size).with(LexError::UnKnowenEscape(c))),
				};

				s.push(resolved);
				continue;
			}


			if c=='"' {
				return Ok(self.yeild_next(size).with(Token::Str(s)));
			}

			if c=='\\' {
				skip = true;
				continue;
			}

			s.push(c);

		}

		Err(self.yeild_next(size).with(LexError::MissingStringClose))
	}

	// fn parse_operator(&mut self) -> Located<Token<'a>> {
	//     // Some ASCII multi-char operators need to be special-cased
	//     let size = match self.cur_str.as_bytes().get(..2) {
	//         Some(b"==") | Some(b"!=") |
	//         Some(b"<=") | Some(b">=") |
	//         Some(b"->") | Some(b"=>") |
	//         Some(b"&&") | Some(b"||") |
	//         Some(b"<<") | Some(b">>") |
	//         Some(b"+=") | Some(b"-=") |
	//         Some(b"*=") | Some(b"/=") |
	//         Some(b"%=") | Some(b"&=") |
	//         Some(b"|=") | Some(b"^=") |
	//         Some(b"::") | Some(b"|>") |
	//         Some(b"++") | Some(b"--") => 2,

	//         _ => self.cur_str.chars().next().unwrap().len_utf8(),
	//     };

	//     self.yeild_next(size).map_owned(Token::Name)
	// }
	fn parse_operator(&mut self) -> Option<Located<Token<'a>>> {
	    
	    match self.cur_str.as_bytes().get(0)?{
	    	b'?'|b'!'|b'.'|b','|
	    	b'*'|b';'|b'\''|
	    	b'('|b')'|b'['|b']'|b'{'|b'}'
	    	=>{
	    		return Some(self.yeild_next(1).map_owned(Token::Name));
	    	}
	    	_=>{}
	    }

	    let mut size = 0usize;
		for c in self.cur_str.chars(){
			if c.is_alphanumeric() || c=='_' || c=='"' || c.is_whitespace() {
				break;
			}

			size+=c.len_utf8();
		}

		Some(self.yeild_next(size).map_owned(Token::Name))
	}


	pub fn next(&mut self)->LexRes<Option<Located<Token<'a>>>>{
		self.skip_comments();
		let Some(c) = self.cur_str.chars().next() else{
			return Ok(None);
		};
		
		if c == '"'{
			return Ok(Some(self.parse_string()?))	
		}
		if c.is_alphabetic() || c=='_' {
			return Ok(Some(self.parse_name()));
		}
		if c.is_numeric(){
			return Ok(Some(self.parse_number()?));
		}


		Ok(self.parse_operator())
	}
}



pub type Bp = i32;

#[derive(Debug, Error, Clone)]
pub enum ParseError<'a> {
    #[error("{0}")]
    Lex(LexError),

    #[error("unknown name: \"{0}\"")]
    UnknownName(&'a str),

    #[error("\"{0}\" is not a value or prefix operator")]
    MissingPrefix(&'a str),

    #[error("expected a value after this found end of expression")]
    ExpectedValue,
}

pub type ParseRes<'a, T = LocAst> = Result<T, Located<ParseError<'a>>>;
pub type ParseOpRes<'a, T = LocAst> = ParseRes<'a, Option<T>>;

impl From<LexError> for ParseError<'_>{
fn from(e: LexError) -> Self {Self::Lex(e)}
}


#[derive(Debug, PartialEq)]
pub struct OpCall(pub Vec<LocAst>);
impl OpCall {
	pub fn new(rator:LocAst)->Self{
		Self(vec![rator])
	}

	pub fn push(&mut self,rand:LocAst){
		self.0.push(rand)
	}

	pub fn rator(&self)->&LocAst{
		&self.0[0]
	}

	pub fn rands(&self)->&[LocAst]{
		&self.0[1..]
	}
}

#[cfg(target_pointer_width = "64")]
#[repr(align(32))]
#[derive(Debug, PartialEq)]
pub enum Ast{
	Op(OpCall),
	StringLit(String),
	IntLit(u64),
	SignedInt(i64),
	Var(VarID),
	GlobalVar(GVarID),//can be function
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(
        size_of::<Ast>() == 32,
        "not ideal..."
    );
};

pub type LocAst = Located<Ast>;


#[derive(Debug,Default)]
pub struct Lexer<'a>{
	pub stack: Vec<BasicLexer<'a>>,
	pub saved_peek: Option<Located<Token<'a>>>,
}

impl<'a> Lexer<'a> {
	pub fn push(&mut self,text:&'a str,src:Source){
		self.stack.push(BasicLexer::new(text,src))
	}

	pub fn new(cur_str:&'a str,src:Source)->Self{
		let mut ans = Self::default();
		ans.push(cur_str,src);
		ans
	}

	pub fn peek(&mut self)->ParseOpRes<'a,&Located<Token<'a>>>{
		if self.saved_peek.is_none() {
			self.saved_peek = self.next()?;
		}

		Ok(self.saved_peek.as_ref())
	}

	pub fn next(&mut self)->ParseOpRes<'a,Located<Token<'a>>>{
		if self.saved_peek.is_some(){
			return Ok(self.saved_peek.take());
		}
		let Some(lex) = self.stack.last_mut() else {
			return Ok(None);
		};

		if let Some(ans) = lex.next().map_err(|e|e.fixtype())?  {
			return Ok(Some(ans));
		}

		self.stack.pop();
		self.next()
	}
}


pub trait PrefixParse {
	fn parse<'a>(&self,my_loc:Loc,parser:&mut Parser<'_,'a>)->ParseRes<'a>;
}

pub struct BasicPrefix{
	pub id:GVarID,
	pub bp:Bp,
}

impl PrefixParse for BasicPrefix {
	fn parse<'a>(&self,my_loc:Loc,parser:&mut Parser<'_,'a>)->ParseRes<'a>{
		let Some(lhs) = parser.expr_bp(self.bp)? else {
			return Err(my_loc.with(ParseError::ExpectedValue))
		};
		let loc = parser.ctx.combine_locs(lhs.loc,my_loc).expect("bad parse stack");
		
		let mut ans = OpCall::new(my_loc.with(Ast::GlobalVar(self.id)));
		ans.push(lhs);

		Ok(loc.with(Ast::Op(ans)))
	}

}

pub trait InfixOp {
	fn combine(&self,my_loc:Loc,lhs:LocAst,rhs:LocAst)->ParseRes<'static,Ast>;
	fn bp(&self)->Bp;
}

pub struct BasicInfix {
	pub id:GVarID,
	pub bp:Bp,
}

impl InfixOp for BasicInfix {
	fn combine(&self,my_loc:Loc,lhs:LocAst,rhs:LocAst)->ParseRes<'static,Ast>{
		let mut ans = OpCall::new(my_loc.with(Ast::GlobalVar(self.id)));
		ans.push(lhs);
		ans.push(rhs);
		Ok(Ast::Op(ans))
	}
	fn bp(&self)->Bp{
		self.bp
	}
}

pub trait PostfixOp {
	fn parse<'a>(&self,my_loc:Loc,lhs:LocAst,parser:&mut Parser<'_,'a>)->ParseRes<'a>;
	fn bp(&self)->Bp;
}

pub struct BasicPostfix{
	pub id:GVarID,
	pub bp:Bp,
}

impl PostfixOp for BasicPostfix {
	fn parse<'a>(&self,my_loc:Loc,lhs:LocAst,parser:&mut Parser<'_,'a>)->ParseRes<'a>{
		let loc = parser.ctx.combine_locs(lhs.loc,my_loc).expect("bad parse stack");
		
		let mut ans = OpCall::new(my_loc.with(Ast::GlobalVar(self.id)));
		ans.push(lhs);

		Ok(loc.with(Ast::Op(ans)))
	}
	fn bp(&self)->Bp{
		self.bp
	}
}

pub enum PostParse {
	Infix(Box<dyn InfixOp>),
	Postfix(Box<dyn PostfixOp>),
}

pub struct ParseOptions {
	pub pre:Option<Box<dyn PrefixParse>>,
	pub post:Option<PostParse>,
}

type KnowenName = Rc<ParseOptions>;


pub struct Scope<'b,K,V>{
	pub owned:HashMap<K,V>,
	pub parent:Option<&'b Scope<'b,K,V>>,
}

impl<K:Hash+Eq, V> Scope<'_, K, V>{
	pub fn get(&self,key:K)->Option<&V>{
		match self.owned.get(&key){
			Some(x)=>Some(x),
			None=>self.parent?.get(key)
		}
	}

	pub fn get_ref(&self,key:&K)->Option<&V>{
		match self.owned.get(key){
			Some(x)=>Some(x),
			None=>self.parent?.get_ref(key)
		}
	}
}

pub struct Parser<'me,'a> {
	pub lexer:Lexer<'a>,
	pub names:Scope<'me,&'a str,KnowenName>,
	pub ctx:&'a SourceContext,
}

impl GVarID {
    // ----- Arithmetic (binary) -----
    pub const ADD_BIN:    Self = GVarID(1);   // +
    pub const SUB_BIN:    Self = GVarID(2);   // -
    pub const MUL_BIN:    Self = GVarID(3);   // *
    pub const DIV_BIN:    Self = GVarID(4);   // /

    // ----- Unary prefix -----
    pub const NEG_PRE:    Self = GVarID(10);  // -x
    pub const DEREF_PRE:  Self = GVarID(11);  // *x
    pub const NOT_PRE:    Self = GVarID(12);  // !x

    // ----- Bitwise (binary) -----
    pub const BIT_AND:    Self = GVarID(20);  // &
    pub const BIT_OR:     Self = GVarID(21);  // |
    pub const BIT_XOR:    Self = GVarID(22);  // ^
    pub const SHL:        Self = GVarID(23);  // <<
    pub const SHR:        Self = GVarID(24);  // >>

    // ----- Logical (binary) -----
    pub const LOG_AND:    Self = GVarID(30);  // &&
    pub const LOG_OR:     Self = GVarID(31);  // ||

    // ----- Assignment -----
    pub const ASSIGN:     Self = GVarID(40);  // =

    // ----- Access -----
    pub const DOT:        Self = GVarID(41);  // .

    // ----- Postfix -----
    pub const QMARK_POST: Self = GVarID(42);  // ?
}





impl<'me, 'a> Parser<'me, 'a> {
    pub fn new_default(lexer: Lexer<'a>, ctx: &'a SourceContext) -> Self {
        let mut owned = HashMap::new();

        macro_rules! prefix {
            ($name:expr, $id:expr, $bp:expr) => {
                owned.insert(
                    $name,
                    Rc::new(ParseOptions {
                        pre:  Some(Box::new(BasicPrefix { id: $id, bp: $bp })),
                        post: None,
                    }),
                );
            };
        }
        macro_rules! infix {
            ($name:expr, $id:expr, $bp:expr) => {
                owned.insert(
                    $name,
                    Rc::new(ParseOptions {
                        pre: None,
                        post: Some(PostParse::Infix(Box::new(BasicInfix { id: $id, bp: $bp }))),
                    }),
                );
            };
        }
        macro_rules! both_ids {
            ($name:expr, $pre_id:expr, $pre_bp:expr, $in_id:expr, $in_bp:expr) => {
                owned.insert(
                    $name,
                    Rc::new(ParseOptions {
                        pre:  Some(Box::new(BasicPrefix { id: $pre_id, bp: $pre_bp })),
                        post: Some(PostParse::Infix(Box::new(BasicInfix { id: $in_id, bp: $in_bp }))),
                    }),
                );
            };
        }
        macro_rules! postfix {
            ($name:expr, $id:expr, $bp:expr) => {
                owned.insert(
                    $name,
                    Rc::new(ParseOptions {
                        pre: None,
                        post: Some(PostParse::Postfix(Box::new(BasicPostfix { id: $id, bp: $bp }))),
                    }),
                );
            };
        }

        // Binding powers (higher = tighter)
        // 80: .
        // 75: postfix (?)
        // 70: prefix (!, -, *)
        // 60: * /
        // 55: << >>
        // 50: + -
        // 45: &
        // 42: ^
        // 40: |
        // 30: &&
        // 25: ||
        // 20: =

        // Arithmetic
        infix!("+",  GVarID::ADD_BIN,    50);                // (no unary +)
        both_ids!("-", GVarID::NEG_PRE,  70, GVarID::SUB_BIN, 50);
        both_ids!("*", GVarID::DEREF_PRE,70, GVarID::MUL_BIN, 60);
        infix!("/",  GVarID::DIV_BIN,    60);

        // Bitwise
        infix!("&",  GVarID::BIT_AND,    45);
        infix!("^",  GVarID::BIT_XOR,    42);
        infix!("|",  GVarID::BIT_OR,     40);
        infix!("<<", GVarID::SHL,        55);
        infix!(">>", GVarID::SHR,        55);

        // Logical
        prefix!("!",  GVarID::NOT_PRE,   70);
        infix!("&&",  GVarID::LOG_AND,   30);
        infix!("||",  GVarID::LOG_OR,    25);

        // Assignment
        infix!("=",   GVarID::ASSIGN,    20);

        // Dot access
        infix!(".",   GVarID::DOT,       80);

        // Postfix
        postfix!("?", GVarID::QMARK_POST, 75);

        let names = Scope { owned, parent: None };
        Self { lexer, names, ctx }
    }


	#[inline(always)]
	pub fn parse_exp(&mut self)->ParseOpRes<'a>{
		self.expr_bp(Bp::MIN)
	}
	pub fn expr_bp(&mut self, min_bp: Bp) -> ParseOpRes<'a> {
	    // println!("new entry");

	    let Some(tok) = self.lexer.next()? else {
	        return Ok(None);
	    };

	    // --- prefix / atom phase ---
	    let mut lhs: LocAst = match tok.value {
	        Token::Str(s) => tok.loc.with(Ast::StringLit(s.into())),
	        Token::Num(i) => tok.loc.with(Ast::IntLit(i)),
	        Token::Name(n) => {
	            let name = tok.with(n); // Located<&str>

	            // Lookup in operator table
	            let Some(opts) = self.names.get(name.value) else {
	                return Err(name.with(ParseError::UnknownName(name.value)));
	            };

	            // println!("runing prefix op");

	            // We have a known name, but check if it’s a prefix op
	            match opts.clone().pre.as_ref() {
	                Some(pre) => pre.parse(tok.loc,self)?,
	                None => return Err(name.with(ParseError::MissingPrefix(name.value))),
	            }
	        }
	    };

	    // println!("starting loop");

	    // --- postfix / infix loop ---
	    loop {
	        let Some(peek_tok) = self.lexer.peek()? else {
	            // println!("no more input");
	            break;
	        };

	        let name = match &peek_tok.value {
	            Token::Name(n) => peek_tok.with(*n),
	            _ => break, // not an operator
	        };

	        let Some(opts) = self.names.get(name.value) else {
	            break; // unknown name -> not an operator
	        };

	        let opts = opts.clone();

	        let Some(post) = &opts.post else {
	            break; // no postfix/infix handler
	        };

	        // println!("found postop");

	        let l_bp = match post {
	            PostParse::Postfix(op) => op.bp(),
	            PostParse::Infix(op) => op.bp(),
	        };

	        if l_bp < min_bp {
	            break;
	        }

	        let op_tok = self.lexer.next()?.unwrap(); // consume operator token

	        lhs = match post {
	            PostParse::Postfix(op) => op.parse(op_tok.loc,lhs, self)?,
	            PostParse::Infix(op) => {
	                let rhs = self
	                    .expr_bp(l_bp)?
	                    .ok_or(op_tok.with(ParseError::ExpectedValue))?;


	                let loc = self.ctx.combine_locs(lhs.loc,rhs.loc).expect("bad spans in lex stack");
	                let val = op.combine(op_tok.loc,lhs, rhs)?;

	                loc.with(val)
	            }
	        };
	    }

	    Ok(Some(lhs))
	}

}


use crate::input::FileId;

#[test]
fn test_lexer(){
	let src = Source::File(FileId(0));
	let text = "hi there \" hi there \\\" \" //blablabla\n ; => weee 1_32_2 ";

	let mut lex = BasicLexer::new(text, src);

	let mut toks = Vec::new();
	while let Some(t) = lex.next().unwrap(){
		toks.push(t);
	}

	assert_eq!(toks.last().unwrap().value,Token::Num(1322));
	assert_eq!(toks.len(),7);
}

#[test]
fn test_lexer_qmark(){
	
	let src = Source::File(FileId(0));
	let text = r#""a"?"#;

	let mut lex = BasicLexer::new(text, src);

	let mut toks = Vec::new();
	while let Some(t) = lex.next().unwrap(){
		toks.push(t);
	}

	assert_eq!(toks.len(),2);
	assert_eq!(toks.last().unwrap().value,Token::Name("?"));

}

#[cfg(test)]
mod parser_tests {
    use super::*;

    macro_rules! parse_single {
        ($src_text:expr) => {{
            let src = Source::File(FileId(0));
            let lexer = Lexer::new($src_text, src);
            let ctx = SourceContext::new();
            let mut parser = Parser::new_default(lexer, &ctx);
            match parser.parse_exp() {
                Ok(Some(ast)) => ast,
                Ok(None) => panic!("no expression parsed in {:?}", $src_text),
                Err(e) => panic!("parse error: {:?}", e),
            }
        }};
    }

    macro_rules! assert_global {
        ($ast:expr, $expected:expr) => {{
            match $ast {
                Ast::GlobalVar(id) => assert_eq!(
                    *id, $expected,
                    "expected {:?}, got {:?}",
                    $expected, id
                ),
                other => panic!(
                    "expected Ast::GlobalVar({:?}), got {:?}",
                    $expected, other
                ),
            }
        }};
    }

    #[test]
    fn parses_simple_number() {
        let ast = parse_single!("42");
        println!("got {ast:?}");
        match ast.value {
            Ast::IntLit(v) => assert_eq!(v, 42),
            other => panic!("expected IntLit(42), got {:?}", other),
        }
    }

    #[test]
    fn parses_prefix_minus() {
        let ast = parse_single!("-5");
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator().value, GVarID::NEG_PRE);
                match &call.rands()[0].value {
                    Ast::IntLit(v) => assert_eq!(*v, 5),
                    other => panic!("expected IntLit(5), got {:?}", other),
                }
            }
            other => panic!("expected operator AST, got {:?}", other),
        }
    }

    #[test]
    fn parses_prefix_and_infix_star() {
        let ast = parse_single!(r#"*"x" * "y""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator().value, GVarID::MUL_BIN);
                let lhs = &call.rands()[0];
                let rhs = &call.rands()[1];
                match &lhs.value {
                    Ast::Op(inner) => assert_global!(&inner.rator().value, GVarID::DEREF_PRE),
                    other => panic!("expected prefix *, got {:?}", other),
                }
                match &rhs.value {
                    Ast::StringLit(_) => {}
                    other => panic!("expected StringLit, got {:?}", other),
                }
            }
            other => panic!("expected infix * at root, got {:?}", other),
        }
    }

    #[test]
    fn parses_double_prefix_and_infix_chain() {
        let ast = parse_single!(r#"-"a" * -"b" + "c""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator().value, GVarID::ADD_BIN);
                let mul_node = &call.rands()[0];
                if let Ast::Op(inner) = &mul_node.value {
                    assert_global!(&inner.rator().value, GVarID::MUL_BIN);
                    let left = &inner.rands()[0];
                    let right = &inner.rands()[1];
                    for operand in [left, right] {
                        match &operand.value {
                            Ast::Op(sub_inner) => {
                                assert_global!(&sub_inner.rator().value, GVarID::NEG_PRE);
                            }
                            other => panic!("expected unary -, got {:?}", other),
                        }
                    }
                } else {
                    panic!("expected * node, got {:?}", mul_node.value);
                }
            }
            other => panic!("expected + at root, got {:?}", other),
        }
    }

    #[test]
    fn parses_dot_access_precedence() {
        let ast = parse_single!(r#""a"."b" + "c"."d""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator().value, GVarID::ADD_BIN);
                for side in call.rands() {
                    match &side.value {
                        Ast::Op(inner) => assert_global!(&inner.rator().value, GVarID::DOT),
                        other => panic!("expected dot access, got {:?}", other),
                    }
                }
            }
            other => panic!("expected top-level +, got {:?}", other),
        }
    }

    #[test]
    fn parses_bitwise_ops() {
        let ast = parse_single!(r#""a" & "b" | "c" ^ "d""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(or) => {
                assert_global!(&or.rator().value, GVarID::BIT_OR);
                let left = &or.rands()[0];
                let right = &or.rands()[1];
                if let Ast::Op(and) = &left.value {
                    assert_global!(&and.rator().value, GVarID::BIT_AND);
                } else {
                    panic!("expected & inside left, got {:?}", left.value);
                }
                if let Ast::Op(xor) = &right.value {
                    assert_global!(&xor.rator().value, GVarID::BIT_XOR);
                } else {
                    panic!("expected ^ inside right, got {:?}", right.value);
                }
            }
            other => panic!("expected | as root, got {:?}", other),
        }
    }

    // #[test]
    // fn parses_logical_and_grouping() {
    //     let ast = parse_single!(r#"!("a" && "b") || "c""#);
    //     println!("got {ast:?}");
    //     match &ast.value {
    //         Ast::Op(or_op) => {
    //             assert_global!(&or_op.rator().value, GVarID::LOG_OR);
    //             let left = &or_op.rands()[0];
    //             if let Ast::Op(not_op) = &left.value {
    //                 assert_global!(&not_op.rator().value, GVarID::NOT_PRE);
    //             } else {
    //                 panic!("expected prefix !, got {:?}", left.value);
    //             }
    //         }
    //         other => panic!("expected || as root, got {:?}", other),
    //     }
    // }

    #[test]
    fn parses_assignment() {
        let ast = parse_single!(r#""x" = "y""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(assign) => assert_global!(&assign.rator().value, GVarID::ASSIGN),
            other => panic!("expected assignment at root, got {:?}", other),
        }
    }

    #[test]
    fn parses_postfix_qmark() {
        let ast = parse_single!(r#""a"?"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator().value, GVarID::QMARK_POST);
                match &call.rands()[0].value {
                    Ast::StringLit(_) => {}
                    other => panic!("expected StringLit, got {:?}", other),
                }
            }
            other => panic!("expected postfix ? AST, got {:?}", other),
        }
    }

    #[test]
    fn parses_mixed_precedence_chain() {
        let ast = parse_single!(r#""a" + "b" << "c"?"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(add) => {
                assert_global!(&add.rator().value, GVarID::ADD_BIN);
                let rhs = &add.rands()[1];
                if let Ast::Op(shift) = &rhs.value {
                    assert_global!(&shift.rator().value, GVarID::SHL);
                    let post = &shift.rands()[1];
                    if let Ast::Op(postfix) = &post.value {
                        assert_global!(&postfix.rator().value, GVarID::QMARK_POST);
                    } else {
                        panic!("expected postfix ? inside rhs, got {:?}", post.value);
                    }
                } else {
                    panic!("expected << node inside + rhs, got {:?}", rhs.value);
                }
            }
            other => panic!("expected + as root, got {:?}", other),
        }
    }
}
