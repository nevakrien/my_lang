# my_lang (place holder)
trying to make a PL for real

# Goal
I am aiming at a PL which is systems-level, safe, simple, debugble, and powerful.
this is fairly ambicious and I doubt all 5 are possible but its still worth trying.

# Progress
a basic parser is already implemented for expressions.
it is fairly extensible capable of handeling syntax extentions at runtime.

error reporting is fairly solid with line numbers included.
there are a few places where it could be improved but for a first draft this is very solid.

the code is thread safe with costs being fairly exceptble (some edge cases of weird macro expantions are slightly more expensive than needed)

# Design 

a few core ideas and somewhat unorthodox decisons are made here.
main premise is an idea of mine i would like to coin as "backend code".
this is code that is ran directly on IR where an optimization pass would usually go.
howver it is fully exposed to the user of the PL.

the diffrence to macros is that backend code is ran **after** type resolution has finished.
which allows for a much more targeted aproch.

the hope is that the entire LLVM backend could be written as pure backend code and live entirly user side.



the second choice is choosing C style text macros. this is for a few reasons:

1. they are simpler in concept to an AST and do not require any languge specific knowledge
2. they do not force me to expose the AST to the PL
3. they allow completly external tools to be used in the macro (like just reading a file)

ideally a similarly simple API could be applied for backend code.
as an example having just compile time excutions solves the ABI problem. but it adds problems of its own around having multiple passes.


