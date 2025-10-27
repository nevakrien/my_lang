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
            loc:Location::Simple(self)
        }
    }
}


#[repr(C,u32)]
#[derive(Clone,PartialEq,Debug)]
pub enum Location {
	Simple(Loc),
	Many(Rc<[Loc]>)
}

impl Location {
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
    pub loc: Location,
}

impl<T> Located<T> {
    #[inline]
    pub fn new(value: T, loc: Location) -> Self {
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

		let ans = Located::new(value,Location::Simple(loc));

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
	fn parse_operator(&mut self) -> Located<Token<'a>> {
	    
	    match self.cur_str.as_bytes().get(1).unwrap(){
	    	b'?'|b'!'|b'.'|
	    	b'*'|b';'|b'\''|
	    	b'('|b')'|b'['|b']'|b'{'|b'}'
	    	=>{
	    		return self.yeild_next(1).map_owned(Token::Name);
	    	}
	    	_=>{}
	    }

	    let mut size = 0usize;
		for c in self.cur_str.chars(){
			if c.is_alphanumeric() || c=='_' || c.is_whitespace() {
				break;
			}

			size+=c.len_utf8();
		}

		self.yeild_next(size).map_owned(Token::Name)
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


		Ok(Some(self.parse_operator()))
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

    #[error("expected operand")]
    ExpectedOperand,
}

pub type ParseRes<'a, T = LocValue> = Result<T, Located<ParseError<'a>>>;
pub type ParseOpRes<'a, T = LocValue> = ParseRes<'a, Option<T>>;

impl From<LexError> for ParseError<'_>{
fn from(e: LexError) -> Self {Self::Lex(e)}
}




#[derive(Debug, PartialEq)]
pub enum Value{
	Op(Box<LocValue>,Vec<LocValue>),
	StringLit(String),
	IntLit(u64),
	SignedInt(i64),
	Var(VarID),
	GlobalVar(VarID),//can be function
}

pub type LocValue = Located<Value>;

// #[derive(Debug, PartialEq)]
// pub enum Value {
// 	StringLit(String),
// 	IntLit(u64),
// }



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
	fn parse<'a>(&self,parser:&mut Parser<'_,'a>)->ParseRes<'a>;
	fn bp(&self)->Bp;
}

pub trait InfixOp {
	fn combine(&self,lhs:LocValue,rhs:LocValue)->ParseRes<'static>;
	fn bp(&self)->Bp;
}

pub trait PostfixOp {
	fn parse<'a>(&self,lhs:LocValue,parser:&mut Parser<'_,'a>)->ParseRes<'a>;
	fn bp(&self)->Bp;
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
	names:Scope<'me,&'a str,KnowenName>,
}


impl<'me,'a> Parser<'me,'a>{
	 pub fn new_defualt(lexer: Lexer<'a>) -> Self {
        let mut owned = HashMap::new();
        let names = Scope{
        	owned,
        	parent:None
        };
        

        Self { lexer, names }
    }



	#[inline(always)]
	pub fn parse_exp(&mut self)->ParseOpRes<'a>{
		self.expr_bp(Bp::MIN)
	}
	pub fn expr_bp(&mut self, min_bp: Bp) -> ParseOpRes<'a> {
	    let Some(tok) = self.lexer.next()? else {
	        return Ok(None);
	    };

	    // --- prefix / atom phase ---
	    let mut lhs: LocValue = match tok.value {
	        Token::Str(s) => tok.loc.with(Value::StringLit(s.into())),
	        Token::Num(i) => tok.loc.with(Value::IntLit(i)),
	        Token::Name(n) => {
	            let name = tok.with(n); // Located<&str>

	            // Lookup in operator table
	            let Some(opts) = self.names.get(name.value) else {
	                return Err(name.with(ParseError::UnknownName(name.value)));
	            };

	            // We have a known name, but check if it’s a prefix op
	            match opts.clone().pre.as_ref() {
	                Some(pre) => pre.parse(self)?,
	                None => return Err(name.with(ParseError::MissingPrefix(name.value))),
	            }
	        }
	    };

	    // --- postfix / infix loop ---
	    loop {
	        let Some(peek_tok) = self.lexer.peek()? else {
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

	        let l_bp = match post {
	            PostParse::Postfix(op) => op.bp(),
	            PostParse::Infix(op) => op.bp(),
	        };

	        if l_bp < min_bp {
	            break;
	        }

	        let op_tok = self.lexer.next()?.unwrap(); // consume operator token

	        lhs = match post {
	            PostParse::Postfix(op) => op.parse(lhs, self)?,
	            PostParse::Infix(op) => {
	                let rhs = self
	                    .expr_bp(l_bp)?
	                    .ok_or(op_tok.with(ParseError::ExpectedOperand))?;
	                op.combine(lhs, rhs)?
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

