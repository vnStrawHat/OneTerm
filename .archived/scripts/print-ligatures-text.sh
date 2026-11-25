
#!/usr/bin/env bash

set -euo pipefail

printf '\n=== Programming Ligatures Test ===\n\n'

# Common programming ligatures
cat <<'EOF'
--- My Test ---
|---|---|
#=
#[
--- Operators ---
!=  !==  =!=  =/=  /=  <>
==  ===  =:=  =!=  <=  >=
<=>  ->  -->  ->>  <-  <--  <-
=>  ==>  <==  <=>  <=>
++  +++  --  ---
&&  &&&  ||  |||
!!  ??  ?.  ?:  ??
::  :::  :=  ::=  :>
..  ...  ..<  .?  .*
**  ***  //  ///
%%  %=  &&=  ||=  <<=  >>=

--- Arrows ---
<-  <--  <---  <----  <->
->  -->  --->  ---->
<==  ==>  <==>  <-->
<~  ~>  <~~  ~~>
|>  <|  ||>  <||

--- Comparisons ---
<  <=  <<  <<<  <|
>  >=  >>  >>>  >|
=  ==  ===  !=  !==

--- Functional / Language-specific ---
|>  <|  >>=  =<<
>>=  >>=  <$>  <*>  <*  *>
::  =>  ->  ~>  :=

--- Brackets and combinations ---
[|  |]  {|  |}  (*  *)
</  />  <!--  -->
${  }  #{  #@

--- Security / Code review ---
user_input == trusted_input
if (x != null && y >= 0) { return true; }
result = foo(bar) => baz(qux);
path = "/api/v1/*";
token != "" && authz == true
EOF

printf '\n=== Unicode symbols (not necessarily ligatures) ===\n\n'

# Unicode glyphs for font and rendering checks
cat <<'EOF'
← → ↔ ⇐ ⇒ ⇔
↖ ↗ ↘ ↙ ↩ ↪
≤ ≥ ≠ ≡ ≢ ≈ ≅
∧ ∨ ¬ ∩ ∪ ∀ ∃
∞ ∑ ∏ √ ∫ ∂ ∇
⊂ ⊃ ⊆ ⊇ ⊕ ⊗
… ⋯ ⋮ ⋱
≪ ≫ ≺ ≻ ⟵ ⟶ ⟷
EOF

printf '\n=== End of test ===\n'
