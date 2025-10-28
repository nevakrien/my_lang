use crate::types::GVarID;
use crate::input::SourceContext;
use crate::types::VarID;
use std::rc::Rc;
use std::hash::Hash;
use thiserror::Error;
use std::collections::HashMap;
// use std::rc::Rc;
use crate::input::Source;
use std::ops::{Deref, DerefMut};

#[derive(Debug,Copy,Clone,PartialEq,Eq,Hash)]
pub struct OpID(pub u32);

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
#[derive(Clone,Copy, Debug, PartialEq)]
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

    #[error("expected \")\" found \"{0}\"")]
    ExpectedParenClose(&'a str),

    #[error("expected \")\" found string")]
    ExpectedParenCloseString,

    #[error("expected \")\" found number")]
    ExpectedParenCloseNum,

    #[error("expected \")\" found EOF")]
    ExpectedParenCloseEOF,
}

pub type ParseRes<'a, T = LocAst> = Result<T, Located<ParseError<'a>>>;
pub type ParseOpRes<'a, T = LocAst> = ParseRes<'a, Option<T>>;

impl From<LexError> for ParseError<'_>{
fn from(e: LexError) -> Self {Self::Lex(e)}
}

#[derive(Debug,PartialEq)]
pub struct OpCall{
	pub rator:Located<OpID>,
	pub rands:Box<[LocAst]>,
}

#[derive(Debug, PartialEq)]
pub enum Ast{
	/// op is distinguished from function style call
	/// this allows for piping and similar ideas
	Op(OpCall),

	/// this is used for functions
	Call(Box<LocAst>,Box<[LocAst]>),
	
	// basics:

	StringLit(String),
	Void,
	IntLit(u64),
	SignedInt(i64),
	Var(VarID),
	GlobalVar(GVarID),//can't be function
}

// #[cfg(target_pointer_width = "64")]
// const _: () = {
//     assert!(
//         size_of::<Ast>() <= 48,
//         "not ideal..."
//     );
// };

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
	pub id:OpID,
	pub bp:Bp,
}

impl PrefixParse for BasicPrefix {
	fn parse<'a>(&self,my_loc:Loc,parser:&mut Parser<'_,'a>)->ParseRes<'a>{
		let Some(lhs) = parser.expr_bp(self.bp)? else {
			return Err(my_loc.with(ParseError::ExpectedValue))
		};
		let loc = parser.ctx.combine_locs(lhs.loc,my_loc).expect("bad parse stack");
		

		Ok(loc.with(Ast::Op(OpCall{
			rator:my_loc.with(self.id),
			rands:[lhs].into()
		})))
	}

}

pub struct ParenPrefix;

fn check_paren_closer_error<'a>(closer:&Located<Token<'a>>)->ParseRes<'a,()>{
	match closer.value {
		Token::Name(")")=>{
			Ok(())
		},
		Token::Name(t)=>{
			Err(closer.loc.with(ParseError::ExpectedParenClose(t)))
		},
		Token::Str(_)=>{
			Err(closer.loc.with(ParseError::ExpectedParenCloseString))
		},
		Token::Num(_)=>{
			Err(closer.loc.with(ParseError::ExpectedParenCloseNum))
		},
	}
}

impl PrefixParse for ParenPrefix {
	fn parse<'a>(&self,my_loc:Loc,parser:&mut Parser<'_,'a>)->ParseRes<'a>{
		//check for ()
		if let Some(loc) = parser.try_name(")")?{
			let loc = parser.ctx.combine_locs(loc,my_loc).expect("bad parse stack");
			return Ok(loc.with(Ast::Void));
		}

		//check for (,)
		if let Some(_) = parser.try_name(",")?{
			let Some(closer) = parser.lexer.next()? else {
				return Err(my_loc.with(ParseError::ExpectedParenCloseEOF))
			};
			check_paren_closer_error(&closer)?;

			let loc = parser.ctx.combine_locs(closer.loc,my_loc).expect("bad parse stack");
			return Ok(loc.with(Ast::Op(OpCall{
				rator:loc.with(OpID::TUPLE),
				rands:[].into(),
			})))
		}



		let Some(lhs) = parser.expr_bp(Bp::MIN)? else {
			return Err(my_loc.with(ParseError::ExpectedValue))
		};

		let Some(closer) = parser.lexer.next()? else {
			return Err(my_loc.with(ParseError::ExpectedParenCloseEOF))
		};

		

		match closer.value {
			Token::Name(")")=>{
				let loc = parser.ctx.combine_locs(closer.loc,my_loc).expect("bad parse stack");
				Ok(loc.with(lhs.value))
			},
			Token::Name(",")=>{
				let mut parts = vec![lhs];
				loop {
					if let Some(_) = parser.check_name(")")?{
						break;
					}

					let Some(exp) = parser.expr_bp(Bp::MIN)? else{
						break;
					};
					parts.push(exp);
					let Some(_) = parser.try_name(",")? else {
						break;
					};

				}
				let Some(closer) = parser.lexer.next()? else {
					return Err(my_loc.with(ParseError::ExpectedParenCloseEOF))
				};
				check_paren_closer_error(&closer)?;

				let loc = parser.ctx.combine_locs(closer.loc,my_loc).expect("bad parse stack");
				Ok(loc.with(Ast::Op(OpCall{
					rator:loc.with(OpID::TUPLE),
					rands:parts.into(),
				})))
			},
			_=>{
				check_paren_closer_error(&closer)?;
				unreachable!();
			},
			
		}

	}
}

pub trait InfixOp {
	fn combine(&self,my_loc:Loc,lhs:LocAst,rhs:LocAst)->ParseRes<'static,Ast>;
	fn bp(&self)->(Bp,Bp);
}

pub struct BasicInfix {
	pub id:OpID,
	pub bps:(Bp,Bp),
}

impl InfixOp for BasicInfix {
	fn combine(&self,my_loc:Loc,lhs:LocAst,rhs:LocAst)->ParseRes<'static,Ast>{
		Ok(Ast::Op(OpCall{
			rator:my_loc.with(self.id),
			rands:[lhs,rhs].into()
		}))
	}
	fn bp(&self)->(Bp,Bp){
		self.bps
	}
}

pub trait PostfixOp {
	fn parse<'a>(&self,my_loc:Loc,lhs:LocAst,parser:&mut Parser<'_,'a>)->ParseRes<'a>;
	fn bp(&self)->Bp;
}

pub struct BasicPostfix{
	pub id:OpID,
	pub bp:Bp,
}

impl PostfixOp for BasicPostfix {
	fn parse<'a>(&self,my_loc:Loc,lhs:LocAst,parser:&mut Parser<'_,'a>)->ParseRes<'a>{
		let loc = parser.ctx.combine_locs(lhs.loc,my_loc).expect("bad parse stack");
		
		Ok(loc.with(Ast::Op(OpCall{
			rator:my_loc.with(self.id),
			rands:[lhs].into()
		})))
	}
	fn bp(&self)->Bp{
		self.bp
	}
}

pub fn parse_arg_list<'a>(
    parser: &mut Parser<'_, 'a>,
    open_loc: Loc,
) -> ParseRes<'a, Box<[LocAst]>> {
    let mut args = Vec::new();

    // Empty argument list: ()
    if let Some(_) = parser.try_name(")")? {
        return Ok(Box::from([]));
    }

    loop {
        // Parse one argument
        let Some(arg) = parser.expr_bp(Bp::MIN)? else {
            return Err(open_loc.with(ParseError::ExpectedValue));
        };
        args.push(arg);

        // Check for comma separator
        if parser.try_name(",")?.is_none() {
            break;
        }
    }

    // Expect closing parenthesis
    let Some(close) = parser.lexer.next()? else {
        return Err(open_loc.with(ParseError::ExpectedParenCloseEOF));
    };
    check_paren_closer_error(&close)?;

    Ok(args.into())
}

pub struct CallPostfix;

impl PostfixOp for CallPostfix {
    fn parse<'a>(
        &self,
        my_loc: Loc,
        lhs: LocAst,
        parser: &mut Parser<'_, 'a>,
    ) -> ParseRes<'a> {
        let args = parse_arg_list(parser, my_loc)?;
        let loc = parser.ctx.combine_locs(lhs.loc, my_loc).expect("bad parse stack");

        Ok(loc.with(Ast::Call(Box::new(lhs), args)))
    }

    fn bp(&self) -> Bp { 80 } // same as dot
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

impl OpID {
    // ----- Arithmetic (binary) -----
    pub const ADD_BIN:    Self = OpID(1);   // +
    pub const SUB_BIN:    Self = OpID(2);   // -
    pub const MUL_BIN:    Self = OpID(3);   // *
    pub const DIV_BIN:    Self = OpID(4);   // /

    // ----- Unary prefix -----
    pub const NEG_PRE:    Self = OpID(10);  // -x
    pub const DEREF_PRE:  Self = OpID(11);  // *x
    pub const ADDR_PRE:  Self = OpID(12);  // &x
    pub const NOT_PRE:    Self = OpID(13);  // !x

    // ----- Bitwise (binary) -----
    pub const BIT_AND:    Self = OpID(20);  // &
    pub const BIT_OR:     Self = OpID(21);  // |
    pub const BIT_XOR:    Self = OpID(22);  // ^
    pub const SHL:        Self = OpID(23);  // <<
    pub const SHR:        Self = OpID(24);  // >>

    // ----- Logical (binary) -----
    pub const LOG_AND:    Self = OpID(30);  // &&
    pub const LOG_OR:     Self = OpID(31);  // ||

    // ----- Assignment -----
    pub const ASSIGN:     Self = OpID(40);  // =
    pub const TUPLE:      Self = OpID(41);  // (x,y,..)

    // ----- Access -----
    pub const DOT:        Self = OpID(42);  // .

    // ----- Postfix -----
    pub const QMARK_POST: Self = OpID(43);  // ?
}





impl<'me, 'a> Parser<'me, 'a> {
    pub fn new_default(lexer: Lexer<'a>, ctx: &'a SourceContext) -> Self {
	    let mut owned = HashMap::new();

	    //parens
	    owned.insert(
            "(",
            Rc::new(ParseOptions {
                pre:  Some(Box::new(ParenPrefix)),
                post: Some(PostParse::Postfix(Box::new(CallPostfix))),
            }),
        );

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
	    // infix_left: (bp, bp + 1)
	    macro_rules! infix_left {
	        ($name:expr, $id:expr, $bp:expr) => {
	            owned.insert(
	                $name,
	                Rc::new(ParseOptions {
	                    pre: None,
	                    post: Some(PostParse::Infix(Box::new(BasicInfix {
	                        id: $id,
	                        bps: ($bp, $bp + 1),
	                    }))),
	                }),
	            );
	        };
	    }
	    // infix_right: (bp - 1, bp)
	    macro_rules! infix_right {
	        ($name:expr, $id:expr, $bp:expr) => {
	            owned.insert(
	                $name,
	                Rc::new(ParseOptions {
	                    pre: None,
	                    post: Some(PostParse::Infix(Box::new(BasicInfix {
	                        id: $id,
	                        bps: ($bp , $bp),
	                    }))),
	                }),
	            );
	        };
	    }
	    // both_ids (prefix + infix_left)
	    macro_rules! both_ids {
	        ($name:expr, $pre_id:expr, $pre_bp:expr, $in_id:expr, $in_bp:expr) => {
	            owned.insert(
	                $name,
	                Rc::new(ParseOptions {
	                    pre:  Some(Box::new(BasicPrefix { id: $pre_id, bp: $pre_bp })),
	                    post: Some(PostParse::Infix(Box::new(BasicInfix {
	                        id: $in_id,
	                        bps: ($in_bp, $in_bp + 1),
	                    }))),
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
	    // 70: prefix (!, -, *, &, +)
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
	    infix_left!("+",  OpID::ADD_BIN,    50);
	    both_ids!("-", OpID::NEG_PRE, 70, OpID::SUB_BIN, 50);
	    both_ids!("*", OpID::DEREF_PRE, 70, OpID::MUL_BIN, 60);
	    infix_left!("/",  OpID::DIV_BIN,    60);

	    // Bitwise
	    both_ids!("&", OpID::ADDR_PRE, 70, OpID::BIT_AND, 45);
	    infix_left!("^",  OpID::BIT_XOR,    42);
	    infix_left!("|",  OpID::BIT_OR,     40);
	    infix_left!("<<", OpID::SHL,        55);
	    infix_left!(">>", OpID::SHR,        55);

	    // Logical
	    prefix!("!",  OpID::NOT_PRE,   70);
	    infix_left!("&&",  OpID::LOG_AND,   30);
	    infix_left!("||",  OpID::LOG_OR,    25);

	    // Assignment – right associative
	    infix_right!("=",   OpID::ASSIGN,    20);

	    // Dot access – left associative
	    infix_left!(".",   OpID::DOT,       80);

	    // Postfix
	    postfix!("?", OpID::QMARK_POST, 75);

	    let names = Scope { owned, parent: None };
	    Self { lexer, names, ctx }
	}

	#[inline]
	pub fn try_name(&mut self,need:&str)->ParseOpRes<'a,Loc>{
		match self.check_name(need)?{
			Some(loc)=>{
				_ = self.lexer.next();
				Ok(Some(loc))
			},
			None=>Ok(None),
		}
	}

	#[inline]
	pub fn check_name(&mut self,need:&str)->ParseOpRes<'a,Loc>{
		let (name,loc) = match self.lexer.peek()?{
			Some(Located{
				value:Token::Name(n),
				loc
			})=>{
				(*n,*loc)
			},
			_=>return Ok(None),
		};

		if name==need{
			// _=self.lexer.next();
			Ok(Some(loc))
		}else{
			Ok(None)
		}

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

	        let (l_bp,r_bp) = match post {
	            PostParse::Postfix(op) => (op.bp(),0/*ignored*/),
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
	                    .expr_bp(r_bp)?
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
	        use std::path::Path;
	        use std::sync::Arc;

	        // Create a SourceContext and register a pseudo file for the test
	        let ctx = SourceContext::new();
	        let path: Arc<Path> = Arc::from(Path::new("test_input.txt"));
	        let file = ctx.get_for_path(path.clone());

	        // Initialize the text cell so errors can be mapped back
	        file.text.set(Ok($src_text.to_string())).unwrap();

	        // Use that file’s Source handle in the lexer
	        let src = Source::File(file.id);
	        let lexer = Lexer::new($src_text, src);
	        let mut parser = Parser::new_default(lexer, &ctx);

	        match parser.parse_exp() {
	            Ok(Some(ast)) => ast,
	            Ok(None) => panic!("no expression parsed in {:?}", $src_text),
	            Err(e) => {
	                // Upgrade with line/column context before panicking
	                let mapped = ctx.add_context(e);
	                panic!("parse error:\n{}", mapped);
	            }
	        }
	    }};
	}


    /// Assert an OpID equals the expected value.
    macro_rules! assert_global {
        ($opid:expr, $expected:expr) => {{
            assert_eq!(
                *$opid, $expected,
                "expected {:?}, got {:?}",
                $expected, $opid
            );
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
                assert_global!(&call.rator.value, OpID::NEG_PRE);
                match &call.rands[0].value {
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
                assert_global!(&call.rator.value, OpID::MUL_BIN);
                let lhs = &call.rands[0];
                let rhs = &call.rands[1];
                match &lhs.value {
                    Ast::Op(inner) => assert_global!(&inner.rator.value, OpID::DEREF_PRE),
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
                assert_global!(&call.rator.value, OpID::ADD_BIN);
                let mul_node = &call.rands[0];
                if let Ast::Op(inner) = &mul_node.value {
                    assert_global!(&inner.rator.value, OpID::MUL_BIN);
                    let left = &inner.rands[0];
                    let right = &inner.rands[1];
                    for operand in [left, right] {
                        match &operand.value {
                            Ast::Op(sub_inner) => {
                                assert_global!(&sub_inner.rator.value, OpID::NEG_PRE);
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
                assert_global!(&call.rator.value, OpID::ADD_BIN);
                for side in &*call.rands {
                    match &side.value {
                        Ast::Op(inner) => assert_global!(&inner.rator.value, OpID::DOT),
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
                assert_global!(&or.rator.value, OpID::BIT_OR);
                let left = &or.rands[0];
                let right = &or.rands[1];
                if let Ast::Op(and) = &left.value {
                    assert_global!(&and.rator.value, OpID::BIT_AND);
                } else {
                    panic!("expected & inside left, got {:?}", left.value);
                }
                if let Ast::Op(xor) = &right.value {
                    assert_global!(&xor.rator.value, OpID::BIT_XOR);
                } else {
                    panic!("expected ^ inside right, got {:?}", right.value);
                }
            }
            other => panic!("expected | as root, got {:?}", other),
        }
    }

    #[test]
    fn parses_logical_and_grouping() {
        let ast = parse_single!(r#"!("a" && "b") || "c""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(or_op) => {
                assert_global!(&or_op.rator.value, OpID::LOG_OR);
                let left = &or_op.rands[0];
                if let Ast::Op(not_op) = &left.value {
                    assert_global!(&not_op.rator.value, OpID::NOT_PRE);
                } else {
                    panic!("expected prefix !, got {:?}", left.value);
                }
            }
            other => panic!("expected || as root, got {:?}", other),
        }
    }

    #[test]
    fn parses_assignment() {
        let ast = parse_single!(r#""x" = "y""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(assign) => assert_global!(&assign.rator.value, OpID::ASSIGN),
            other => panic!("expected assignment at root, got {:?}", other),
        }
    }

    #[test]
    fn parses_postfix_qmark() {
        let ast = parse_single!(r#""a"?"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator.value, OpID::QMARK_POST);
                match &call.rands[0].value {
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
                assert_global!(&add.rator.value, OpID::ADD_BIN);
                let rhs = &add.rands[1];
                if let Ast::Op(shift) = &rhs.value {
                    assert_global!(&shift.rator.value, OpID::SHL);
                    let post = &shift.rands[1];
                    if let Ast::Op(postfix) = &post.value {
                        assert_global!(&postfix.rator.value, OpID::QMARK_POST);
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

    #[test]
    fn parses_assignment_right_associative() {
        // "a" = "b" = "c"
        let ast = parse_single!(r#""a" = "b" = "c""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(outer) => {
                assert_global!(&outer.rator.value, OpID::ASSIGN);
                let lhs = &outer.rands[0];
                let rhs = &outer.rands[1];

                match &rhs.value {
                    Ast::Op(inner) => {
                        assert_global!(&inner.rator.value, OpID::ASSIGN);
                        match &inner.rands[0].value {
                            Ast::StringLit(s) => assert_eq!(s, "b"),
                            other => panic!("expected b, got {:?}", other),
                        }
                        match &inner.rands[1].value {
                            Ast::StringLit(s) => assert_eq!(s, "c"),
                            other => panic!("expected c, got {:?}", other),
                        }
                    }
                    other => panic!("expected nested assignment on RHS, got {:?}", other),
                }

                match &lhs.value {
                    Ast::StringLit(s) => assert_eq!(s, "a"),
                    other => panic!("expected a, got {:?}", other),
                }
            }
            other => panic!("expected assignment at root, got {:?}", other),
        }
    }

    #[test]
    fn parses_addition_left_associative() {
        let ast = parse_single!(r#""a" + "b" + "c""#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(outer) => {
                assert_global!(&outer.rator.value, OpID::ADD_BIN);
                match &outer.rands[0].value {
                    Ast::Op(inner) => assert_global!(&inner.rator.value, OpID::ADD_BIN),
                    other => panic!("expected nested + on LHS, got {:?}", other),
                }
            }
            other => panic!("expected + as root, got {:?}", other),
        }
    }

    #[test]
    fn parses_unit_expression() {
        // ()
        let ast = parse_single!("()");
        println!("got {ast:?}");
        match ast.value {
            Ast::Void => {}
            other => panic!("expected Ast::Void, got {:?}", other),
        }
    }

    #[test]
    fn parses_grouped_expression_not_tuple() {
        // ("a") → just "a"
        let ast = parse_single!(r#"("a")"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::StringLit(s) => assert_eq!(s, "a"),
            other => panic!("expected grouped StringLit(\"a\"), got {:?}", other),
        }
    }

    #[test]
    fn parses_single_element_tuple() {
        // ("a",) → 1-tuple
        let ast = parse_single!(r#"("a",)"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator.value, OpID::TUPLE);
                assert_eq!(call.rands.len(), 1);
                match &call.rands[0].value {
                    Ast::StringLit(s) => assert_eq!(s, "a"),
                    other => panic!("expected single tuple element 'a', got {:?}", other),
                }
            }
            other => panic!("expected tuple Op(TUPLE), got {:?}", other),
        }
    }

    #[test]
    fn parses_multi_element_tuple() {
        // ("a", "b", "c")
        let ast = parse_single!(r#"("a", "b", "c")"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator.value, OpID::TUPLE);
                assert_eq!(call.rands.len(), 3);
                let vals: Vec<_> = call.rands.iter().map(|x| match &x.value {
                    Ast::StringLit(s) => s.as_str(),
                    other => panic!("expected string lit, got {:?}", other),
                }).collect();
                assert_eq!(vals, ["a", "b", "c"]);
            }
            other => panic!("expected tuple Op(TUPLE), got {:?}", other),
        }
    }

    #[test]
    fn parses_nested_tuple_inside_expression() {
        // ("a", ("b", "c"))
        let ast = parse_single!(r#"("a", ("b", "c"))"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator.value, OpID::TUPLE);
                assert_eq!(call.rands.len(), 2);
                match &call.rands[1].value {
                    Ast::Op(inner) => {
                        assert_global!(&inner.rator.value, OpID::TUPLE);
                        assert_eq!(inner.rands.len(), 2);
                    }
                    other => panic!("expected inner tuple as second element, got {:?}", other),
                }
            }
            other => panic!("expected outer tuple, got {:?}", other),
        }
    }

    #[test]
    fn parses_empty_tuple_literal() {
        // "(,)" represents an empty tuple, distinct from void "()"
        let ast = parse_single!("(,)");
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(call) => {
                assert_global!(&call.rator.value, OpID::TUPLE);
                assert!(
                    call.rands.is_empty(),
                    "expected 0 tuple elements, got {:?}",
                    call.rands.len()
                );
            }
            other => panic!("expected Ast::Op(TUPLE) for '(,)', got {:?}", other),
        }
    }

    //CALLS
        #[test]
    fn parses_simple_function_call_empty() {
        // f()
        let ast = parse_single!(r#""f"()"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Call(callee, args) => {
                match &callee.value {
                    Ast::StringLit(s) => assert_eq!(s, "f"),
                    other => panic!("expected callee 'f', got {:?}", other),
                }
                assert!(args.is_empty(), "expected no args, got {:?}", args.len());
            }
            other => panic!("expected Call AST, got {:?}", other),
        }
    }

    #[test]
    fn parses_function_call_with_args() {
        // f("a", "b")
        let ast = parse_single!(r#""f"("a","b")"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Call(callee, args) => {
                assert_eq!(args.len(), 2, "expected 2 args, got {}", args.len());
                match &callee.value {
                    Ast::StringLit(s) => assert_eq!(s, "f"),
                    other => panic!("expected callee 'f', got {:?}", other),
                }
                match &args[0].value {
                    Ast::StringLit(s) => assert_eq!(s, "a"),
                    other => panic!("expected arg1 'a', got {:?}", other),
                }
                match &args[1].value {
                    Ast::StringLit(s) => assert_eq!(s, "b"),
                    other => panic!("expected arg2 'b', got {:?}", other),
                }
            }
            other => panic!("expected Call AST, got {:?}", other),
        }
    }

    #[test]
    fn parses_nested_function_calls() {
        // f(g("x", "y"))
        let ast = parse_single!(r#""f"("g"("x","y"))"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Call(outer_callee, outer_args) => {
                assert_eq!(outer_args.len(), 1);
                match &outer_callee.value {
                    Ast::StringLit(s) => assert_eq!(s, "f"),
                    other => panic!("expected outer callee 'f', got {:?}", other),
                }
                match &outer_args[0].value {
                    Ast::Call(inner_callee, inner_args) => {
                        match &inner_callee.value {
                            Ast::StringLit(s) => assert_eq!(s, "g"),
                            other => panic!("expected inner callee 'g', got {:?}", other),
                        }
                        let arg_names: Vec<_> = inner_args.iter().map(|a| match &a.value {
                            Ast::StringLit(s) => s.clone(),
                            other => panic!("expected string arg, got {:?}", other),
                        }).collect();
                        assert_eq!(arg_names, ["x", "y"]);
                    }
                    other => panic!("expected inner Call AST, got {:?}", other),
                }
            }
            other => panic!("expected outer Call AST, got {:?}", other),
        }
    }

    #[test]
    fn parses_call_and_infix_mixed_precedence() {
        // f("x") + g("y")
        let ast = parse_single!(r#""f"("x") + "g"("y")"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Op(op) => {
                assert_global!(&op.rator.value, OpID::ADD_BIN);
                match &op.rands[0].value {
                    Ast::Call(callee, _) => match &callee.value {
                        Ast::StringLit(s) => assert_eq!(s, "f"),
                        other => panic!("expected left call to 'f', got {:?}", other),
                    },
                    other => panic!("expected left operand call, got {:?}", other),
                }
                match &op.rands[1].value {
                    Ast::Call(callee, _) => match &callee.value {
                        Ast::StringLit(s) => assert_eq!(s, "g"),
                        other => panic!("expected right call to 'g', got {:?}", other),
                    },
                    other => panic!("expected right operand call, got {:?}", other),
                }
            }
            other => panic!("expected + operator at root, got {:?}", other),
        }
    }

    #[test]
    fn parses_chained_calls_associative() {
        // f("a")("b")  →  (f("a"))("b")
        let ast = parse_single!(r#""f"("a")("b")"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Call(outer_callee, outer_args) => {
                assert_eq!(outer_args.len(), 1);
                match &outer_callee.value {
                    Ast::Call(inner_callee, inner_args) => {
                        assert_eq!(inner_args.len(), 1);
                        match &inner_callee.value {
                            Ast::StringLit(s) => assert_eq!(s, "f"),
                            other => panic!("expected inner callee 'f', got {:?}", other),
                        }
                        match &inner_args[0].value {
                            Ast::StringLit(s) => assert_eq!(s, "a"),
                            other => panic!("expected arg 'a', got {:?}", other),
                        }
                    }
                    other => panic!("expected nested Call as callee, got {:?}", other),
                }
                match &outer_args[0].value {
                    Ast::StringLit(s) => assert_eq!(s, "b"),
                    other => panic!("expected arg 'b', got {:?}", other),
                }
            }
            other => panic!("expected Call(AST) chain, got {:?}", other),
        }
    }

    #[test]
    fn parses_call_inside_call_argument() {
        // f(g("a")) → f called with one arg that is g("a")
        let ast = parse_single!(r#""f"("g"("a"))"#);
        println!("got {ast:?}");
        match &ast.value {
            Ast::Call(_outer_callee, outer_args) => {
                assert_eq!(outer_args.len(), 1);
                match &outer_args[0].value {
                    Ast::Call(inner_callee, inner_args) => {
                        assert_eq!(inner_args.len(), 1);
                        match &inner_callee.value {
                            Ast::StringLit(s) => assert_eq!(s, "g"),
                            other => panic!("expected inner callee 'g', got {:?}", other),
                        }
                        match &inner_args[0].value {
                            Ast::StringLit(s) => assert_eq!(s, "a"),
                            other => panic!("expected inner arg 'a', got {:?}", other),
                        }
                    }
                    other => panic!("expected inner Call, got {:?}", other),
                }
            }
            other => panic!("expected outer Call, got {:?}", other),
        }
    }


}
