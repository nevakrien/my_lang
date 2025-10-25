
use thiserror::Error;
use std::fmt;
use std::error::Error;
use std::collections::HashMap;
use std::rc::Rc;
use crate::input::Source;
use crate::input::Loc;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug, PartialEq, Hash)]
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
    		loc:self.loc
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
			size+=c.len_utf8();


			if !(c.is_numeric() || c=='_') {
				let tok = self.yeild_next(size);
				let err = tok.map_owned(|_|{LexError::WeirdNumberEnd(c)});
				return Err(err);
			}

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

	fn parse_operator(&mut self) -> Located<Token<'a>> {
	    // Some ASCII multi-char operators need to be special-cased
	    let size = match self.cur_str.as_bytes().get(..2) {
	        Some(b"==") | Some(b"!=") |
	        Some(b"<=") | Some(b">=") |
	        Some(b"->") | Some(b"=>") |
	        Some(b"&&") | Some(b"||") |
	        Some(b"<<") | Some(b">>") |
	        Some(b"+=") | Some(b"-=") |
	        Some(b"*=") | Some(b"/=") |
	        Some(b"%=") | Some(b"&=") |
	        Some(b"|=") | Some(b"^=") |
	        Some(b"::") |
	        Some(b"++") | Some(b"--") => 2,

	        _ => self.cur_str.chars().next().unwrap().len_utf8(),
	    };

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



pub type Bp = u32;

pub enum ParseError {
	LexError(LexError),
}

impl From<LexError> for ParseError{
fn from(e: LexError) -> Self {Self::LexError(e)}
}

pub enum Value {
	StringLit(String),
	IntLit(u64),
}

pub type ParseOpRes<T=Value> = Result<Option<T>,Located<ParseError>>;
pub type ParseRes<T=Value> = Result<T,Located<ParseError>>;

#[derive(Debug,Default)]
pub struct Lexer<'a>{
	pub stack: Vec<BasicLexer<'a>>,
	pub saved_peek: Option<Located<Token<'a>>>,
}

impl<'a> Lexer<'a> {
	pub fn push(&mut self,text:&'a str,src:Source){
		self.stack.push(BasicLexer::new(text,src))
	}

	pub fn peek(&mut self)->ParseOpRes<&Located<Token<'a>>>{
		if self.saved_peek.is_none() {
			self.saved_peek = self.next()?;
		}

		Ok(self.saved_peek.as_ref())
	}

	pub fn next(&mut self)->ParseOpRes<Located<Token<'a>>>{
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


pub trait ParseElm {
	 fn parse_pre(&self,parser:&mut Parser,min_bp:Bp)->ParseOpRes;
	 fn parse_post(&self,parser:&mut Parser,min_bp:Bp,lhs:Value)->ParseRes;
	 fn postfix_bp(&self)->Option<Bp>;
}


type KnowenName = Rc<dyn ParseElm>;


pub struct Parser<'a> {
	pub lexer:Lexer<'a>,
	names:HashMap<&'a str,KnowenName>,
}

impl<'a> Parser<'a>{
	fn exp_parser(&self,name:&str)->ParseRes<KnowenName>{
		self.names.get(name).ok_or_else(||{todo!()}).cloned()
	}
	#[inline(always)]
	pub fn parse_exp(&mut self)->ParseOpRes{
		self.expr_bp(0)
	}
	pub fn expr_bp(&mut self,min_bp:Bp)->ParseOpRes{
	    let Some(tok) =  self.lexer.next()? else{
	        return Ok(None)
	    };

	    let mut lhs :Value = match tok.value {
	        Token::Str(s)=>Value::StringLit(s),
	        Token::Num(i)=>Value::IntLit(i),
	        Token::Name(n)=> {
	        	let parser = self.exp_parser(n)?;
	        	match parser.parse_pre(self,min_bp)?{
	        		None=>return Ok(None),
	        		Some(x)=>x,
	        	}
	        },
	    };

	    loop {
	        let maybe_op = match self.lexer.peek()?.map(|v|&v.value) {
	            None => break,
	            Some(&Token::Name(n)) => {
	            	self.exp_parser(n)?
	            },
	            Some(_t) => todo!(),//panic!("bad token: {:?}", t),
	        };

	        let Some(l_bp) = maybe_op.postfix_bp() else {
	        	break;
	        };
	        if l_bp < min_bp {
	            break;
	        }

	        self.lexer.next()?;
	        lhs = maybe_op.clone().parse_post(self,min_bp,lhs)?;
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