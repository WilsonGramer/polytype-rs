//! A [Hindley-Milner polymorphic typing
//! system](https://en.wikipedia.org/wiki/Hindley%E2%80%93Milner_type_system).
//!
//! For brevity, the documentation heavily uses the three provided macros when
//! creating types.
//!
//! # Examples
//!
//! The basics:
//!
//! ```
//! # #[macro_use] extern crate polytype;
//! use polytype::Context;
//!
//! # fn main() {
//! // filter: ∀α. (α → bool) → [α] → [α]
//! let t = ptp!(0, ptp!(arrow![
//!     arrow![tp!(0), tp!(bool)],
//!     tp!(list(tp!(0))),
//!     tp!(list(tp!(0))),
//! ]));
//!
//! // quantified types provide polymorphic behavior.
//! assert_eq!(format!("{}", &t), "∀t0. (t0 → bool) → list(t0) → list(t0)");
//! assert!(t.is_polymorphic());
//!
//! // we can instantiate types to remove quantifiers
//! let mut ctx = Context::default();
//! let t = t.instantiate(&mut ctx);
//! assert_eq!(format!("{}", &t), "(t0 → bool) → list(t0) → list(t0)");
//!
//! // we can substitute for t0 using unification in a type Context:
//! ctx.unify(&tp!(0), &tp!(int)).expect("unifies");
//! let t = t.apply(&ctx);
//! assert_eq!(format!("{}", &t), "(int → bool) → list(int) → list(int)");
//! # }
//! ```
//!
//! Extended example:
//!
//! ```
//! # #[macro_use] extern crate polytype;
//! use polytype::Context;
//!
//! # fn main() {
//! // reduce: ∀α. ∀β. (β → α → β) → β → [α] → β
//! // We can represent the type of reduce using the included typing macros:
//! // tp!, ptp!, and arrow!.
//! let t = ptp!(0, ptp!(1, ptp!(arrow![
//!     arrow![tp!(1), tp!(0), tp!(1)],
//!     tp!(1),
//!     tp!(list(tp!(0))),
//!     tp!(1),
//! ])));
//! assert_eq!(format!("{}", &t), "∀t0. ∀t1. (t1 → t0 → t1) → t1 → list(t0) → t1");
//!
//! // reduce is polymorphic
//! assert!(t.is_polymorphic());
//!
//! // let's consider reduce when applied to a function that adds two ints
//!
//! // first, let's create a type representing binary addition.
//! let tplus = arrow![tp!(int), tp!(int), tp!(int)];
//! assert_eq!(format!("{}", &tplus), "int → int → int");
//!
//! // let's also create a new typing context to manage typing bookkeeping.
//! let mut ctx = Context::default();
//!
//! // then, let's instantiate the polymorphic type of reduce within our context
//! // so new type variables will be distinct
//! let t = t.instantiate(&mut ctx);
//! assert_eq!(format!("{}", &t), "(t1 → t0 → t1) → t1 → list(t0) → t1");
//!
//! // by unifying, we can ensure function applications obey type requirements.
//! let treturn = ctx.new_variable();
//! let targ1 = ctx.new_variable();
//! let targ2 = ctx.new_variable();
//! ctx.unify(
//!     &t,
//!     &arrow![
//!         tplus.clone(),
//!         targ1.clone(),
//!         targ2.clone(),
//!         treturn.clone(),
//!     ],
//! ).expect("unifies");
//!
//! // we can also now infer what arguments are needed and what gets returned
//! assert_eq!(targ1.apply(&ctx), tp!(int));             // inferred arg 1: int
//! assert_eq!(targ2.apply(&ctx), tp!(list(tp!(int))));  // inferred arg 2: int
//! assert_eq!(treturn.apply(&ctx), tp!(int));           // inferred return: int
//!
//! // finally, we can see what form reduce takes
//! let t = t.apply(&ctx);
//! assert_eq!(format!("{}", &t), "(int → int → int) → int → list(int) → int");
//! # }
//! ```

extern crate itertools;
#[macro_use]
extern crate nom;

#[macro_use]
mod macros;

use itertools::Itertools;

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// Represents a [type
/// variable](https://en.wikipedia.org/wiki/Hindley%E2%80%93Milner_type_system#Free_type_variables)
/// (an unspecified type)
pub type Variable = u32;

/// Represents
/// [polytypes](https://en.wikipedia.org/wiki/Hindley%E2%80%93Milner_type_system#Polytypes)
/// (universally quantified types).
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Polytype {
    /// Non-polymorphic types (e.g. `α → β`, `int → bool`)
    Monotype(Type),
    /// Polymorphic types (e.g. `∀α. α → α`, `∀α. ∀β. α → β`)
    Binding {
        /// the [`Variable`] being bound
        /// [`Variable`]: type.Variable.html
        variable: Variable,
        /// the type in which `variable` is bound
        body: Box<Polytype>,
    },
}
impl Polytype {
    /// `true` if the type binds type variables else `false`?
    pub fn is_polymorphic(&self) -> bool {
        match *self {
            Polytype::Binding { .. } => true,
            Polytype::Monotype(_) => false,
        }
    }
    /// the work of instantiation happens here.
    fn instantiate_helper(
        &self,
        ctx: &mut Context,
        bindings: &mut HashMap<Variable, Type>,
    ) -> Type {
        match *self {
            Polytype::Monotype(ref t) => t.substitute(bindings),
            Polytype::Binding { variable, ref body } => {
                if let Type::Variable(v) = ctx.new_variable() {
                    bindings.insert(variable, Type::Variable(v));
                }
                body.instantiate_helper(ctx, bindings)
            }
        }
    }
    /// Instantiate a type in the context by removing quantifiers.
    ///
    /// All type variables will be replaced with fresh type variables.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Context;
    /// let mut ctx = Context::default();
    ///
    /// let t1 = ptp!(3, ptp!(tp!(list(tp!(3)))));
    /// let t2 = ptp!(3, ptp!(tp!(list(tp!(3)))));
    ///
    /// let t1 = t1.instantiate(&mut ctx);
    /// let t2 = t2.instantiate(&mut ctx);
    /// assert_eq!(format!("{}", &t1), "list(t0)");
    /// assert_eq!(format!("{}", &t2), "list(t1)");
    /// # }
    /// ```
    pub fn instantiate(&self, ctx: &mut Context) -> Type {
        self.instantiate_helper(ctx, &mut HashMap::new())
    }
}
impl fmt::Display for Polytype {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            Polytype::Binding { variable, ref body } => write!(f, "∀t{}. {}", variable, body),
            Polytype::Monotype(ref t) => t.fmt(f),
        }
    }
}

/// [`Type`] is easier to keyboard, but we provide [`Monotype`] if that helps you maintain the semantic distinction more easily.
///
/// [`Type`]: enum.Type.html
/// [`Monotype`]: type.Monotype.html
pub type Monotype = Type;

/// Represents
/// [monotypes](https://en.wikipedia.org/wiki/Hindley%E2%80%93Milner_type_system#Monotypes)
/// (unquantified types).
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Type {
    /// primitive or composite types (e.g. `int`, `List(α)`, `α → β`)
    ///
    /// # Examples
    ///
    /// Primitives have no associated types:
    ///
    /// ```
    /// # use polytype::Type;
    /// let tint = Type::Constructed("int", vec![]);
    /// assert_eq!(format!("{}", &tint), "int")
    /// ```
    ///
    /// Composites have associated types:
    ///
    /// ```
    /// # use polytype::Type;
    /// let tint = Type::Constructed("int", vec![]);
    /// let tlist_of_ints = Type::Constructed("list", vec![tint]);
    /// assert_eq!(format!("{}", &tlist_of_ints), "list(int)");
    /// ```
    ///
    /// With the macros:
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// let t = tp!(list(tp!(int)));
    /// assert_eq!(format!("{}", &t), "list(int)");
    /// # }
    /// ```
    Constructed(&'static str, Vec<Type>),
    /// type variables (e.g. `α`, `β`) identified by de Bruin indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Type;
    /// // any function: α → β
    /// let t = arrow![Type::Variable(0), Type::Variable(1)];
    /// assert_eq!(format!("{}", &t), "t0 → t1");
    /// # }
    /// ```
    ///
    /// With the macros:
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// // map: (α → β) → [α] → [β]
    /// let t = arrow![
    ///     arrow![tp!(0), tp!(1)],
    ///     tp!(list(tp!(0))),
    ///     tp!(list(tp!(1))),
    /// ];
    /// assert_eq!(format!("{}", &t), "(t0 → t1) → list(t0) → list(t1)");
    /// # }
    /// ```
    Variable(Variable),
}
impl Type {
    /// shortcut for constructing function types (i.e. `alpha` → `beta`).
    pub fn arrow(alpha: Type, beta: Type) -> Type {
        Type::Constructed("→", vec![alpha, beta])
    }
    /// `true` if `Type` is an arrow type else `false`.
    pub fn is_arrow(&self) -> bool {
        if let &Type::Constructed("→", ..) = self {
            true
        } else {
            false
        }
    }
    fn occurs(&self, v: Variable) -> bool {
        match *self {
            Type::Constructed(_, ref args) => args.iter().any(|t| t.occurs(v)),
            Type::Variable(n) => n == v,
        }
    }
    /// Supplying `is_return` helps arrows look cleaner.
    fn show(&self, is_return: bool) -> String {
        match *self {
            Type::Variable(v) => format!("t{}", v),
            Type::Constructed(name, ref args) => {
                if args.is_empty() {
                    String::from(name)
                } else if name == "→" {
                    Type::arrow_show(args, is_return)
                } else {
                    format!("{}({})", name, args.iter().map(|t| t.show(true)).join(","))
                }
            }
        }
    }
    /// show specifically for arrow types
    fn arrow_show(args: &Vec<Type>, is_return: bool) -> String {
        if is_return {
            format!("{} → {}", args[0].show(false), args[1].show(true))
        } else {
            format!("({} → {})", args[0].show(false), args[1].show(true))
        }
    }
    /// Optionally return the arguments of an arrow.
    pub fn args(&self) -> Option<VecDeque<&Type>> {
        match *self {
            Type::Variable(_) => None,
            Type::Constructed("→", ref args) => {
                let mut tps = args[1].args().unwrap_or(VecDeque::new());
                tps.push_front(&args[0]);
                Some(tps)
            }
            Type::Constructed(..) => None,
        }
    }
    /// Optionally return the return type of an arrow.
    pub fn returns(&self) -> Option<&Type> {
        match *self {
            Type::Variable(_) => None,
            Type::Constructed("→", ref args) => args[1].returns().or(Some(&args[1])),
            Type::Constructed(..) => None,
        }
    }
    /// Applies the type in a [`Context`].
    ///
    /// This will replace any type variables that have substitutions defined in
    /// the context.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Context;
    /// let mut ctx = Context::default();
    /// ctx.unify(&tp!(0), &tp!(int)).expect("unifies");
    ///
    /// let t = tp!(list(tp!(0)));
    /// assert_eq!(format!("{}", &t), "list(t0)");
    /// let t = t.apply(&ctx);
    /// assert_eq!(format!("{}", &t), "list(int)");
    /// # }
    /// ```
    ///
    /// [`Context`]: struct.Context.html
    pub fn apply(&self, ctx: &Context) -> Type {
        match *self {
            Type::Constructed(name, ref args) => {
                let args = args.iter().map(|t| t.apply(ctx)).collect();
                Type::Constructed(name, args)
            }
            Type::Variable(v) => {
                if let Some(tp) = ctx.substitutions.get(&v) {
                    // hmm... is this right?
                    tp.apply(ctx)
                } else {
                    Type::Variable(v)
                }
            }
        }
    }
    /// Perform a subsitution according to some set of bindings
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Type;
    /// # use std::collections::HashMap;
    /// let t = arrow![tp!(0), tp!(1)];
    /// assert_eq!(format!("{}", &t), "t0 → t1");
    ///
    /// let mut bindings = HashMap::new();
    /// bindings.insert(0, tp!(int));
    /// bindings.insert(1, tp!(bool));
    /// let t = t.substitute(&bindings);
    ///
    /// assert_eq!(format!("{}", t), "int → bool");
    /// # }
    /// ```
    pub fn substitute(&self, bindings: &HashMap<Variable, Type>) -> Type {
        match *self {
            Type::Constructed(name, ref args) => {
                let args = args.iter().map(|t| t.substitute(bindings)).collect();
                Type::Constructed(name, args)
            }
            Type::Variable(v) => bindings.get(&v).unwrap_or(&Type::Variable(v)).clone(),
        }
    }
    /// Generalizes the type by binding free variables.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::{Context, Type};
    /// let t = arrow![tp!(0), tp!(1)];
    /// assert_eq!(format!("{}", &t), "t0 → t1");
    ///
    /// let mut ctx = Context::default();
    /// ctx.extend(0, tp!(int));
    /// let t_gen = t.apply(&ctx).generalize(&ctx);
    ///
    /// assert_eq!(format!("{}", t_gen), "∀t1. int → t1");
    /// # }
    /// ```
    pub fn generalize(&self, ctx: &Context) -> Polytype {
        let fvs = self.free_vars(ctx);
        let mut t = Polytype::Monotype(self.clone());
        for v in fvs.iter() {
            t = Polytype::Binding {
                variable: v.clone(),
                body: Box::new(t),
            };
        }
        t
    }
    /// Compute all the free variables in a type
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::{Context, Type};
    /// # use std::collections::HashSet;
    /// let t = arrow![tp!(0), tp!(1)];
    /// assert_eq!(format!("{}", &t), "t0 → t1");
    ///
    /// let mut ctx = Context::default();
    /// ctx.extend(0, tp!(int));
    /// let fvs_computed = t.free_vars(&ctx);
    /// let fvs_expected = vec![1].into_iter().collect();
    ///
    /// assert_eq!(fvs_computed, fvs_expected);
    /// # }
    /// ```
    pub fn free_vars(&self, ctx: &Context) -> HashSet<Variable> {
        match *self {
            Type::Constructed(_, ref args) => args.iter()
                .flat_map(|a| a.free_vars(ctx).into_iter())
                .collect(),
            Type::Variable(v) => {
                if !ctx.substitutions().contains_key(&v) {
                    let mut s = HashSet::new();
                    s.insert(v);
                    s
                } else {
                    HashSet::new()
                }
            }
        }
    }
    /// Parse a type from a string. This round-trips with [`Display`]. This is a
    /// **leaky** operation and should be avoided wherever possible: names of
    /// constructed types will remain until program termination.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Type;
    /// let t_par = Type::parse("int -> hashmap(str, list(bool))").expect("valid type");
    /// let t_lit = arrow![tp!(int), tp!(hashmap(tp!(str), tp!(list(tp!(bool)))))];
    /// assert_eq!(t_par, t_lit);
    ///
    /// let s = "(t1 → t0 → t1) → t1 → list(t0) → t1";
    /// let t = Type::parse(s).expect("valid type");
    /// let round_trip = format!("{}", &t);
    /// assert_eq!(s, round_trip);
    /// # }
    /// ```
    ///
    /// [`Display`]: https://doc.rust-lang.org/std/fmt/trait.Display.html
    pub fn parse(s: &str) -> Result<Type, ()> {
        parser::parse(s)
    }
}
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.show(true))
    }
}
impl From<VecDeque<Type>> for Type {
    fn from(mut tps: VecDeque<Type>) -> Type {
        match tps.len() {
            0 => panic!("cannot create a type from nothing"),
            1 => tps.pop_front().unwrap(),
            2 => {
                let alpha = tps.pop_front().unwrap();
                let beta = tps.pop_front().unwrap();
                Type::arrow(alpha, beta)
            }
            _ => {
                let alpha = tps.pop_front().unwrap();
                Type::arrow(alpha, tps.into())
            }
        }
    }
}
impl From<Vec<Type>> for Type {
    fn from(tps: Vec<Type>) -> Type {
        Type::from(VecDeque::from(tps))
    }
}

/// Represents errors in unification.
#[derive(Debug, Clone, PartialEq)]
pub enum UnificationError {
    /// `Occurs` happens when occurs checks fail (i.e. a type variable is
    /// unified recursively). The id of the bad type variable is supplied.
    Occurs(Variable),
    /// `Failure` happens when symbols or type variants don't unify because of
    /// structural differences.
    Failure(Type, Type),
}
impl fmt::Display for UnificationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            UnificationError::Occurs(v) => write!(f, "Occurs({})", v),
            UnificationError::Failure(ref t1, ref t2) => {
                write!(f, "Failure({}, {})", t1.show(false), t2.show(false))
            }
        }
    }
}
impl std::error::Error for UnificationError {
    fn description(&self) -> &str {
        "could not unify"
    }
}

/// Context is a type environment, keeping track of substitutions and type
/// variables. Useful for _unifying_ (and inferring) types.
#[derive(Debug, Clone)]
pub struct Context {
    substitutions: HashMap<Variable, Type>,
    next: Variable,
}
impl Default for Context {
    fn default() -> Self {
        Context {
            substitutions: HashMap::new(),
            next: 0,
        }
    }
}
impl Context {
    /// Return a map of bindings managed by the context. Each key is a
    /// [`Variable`], and each value is a [`Type`].
    ///
    /// [`Type`]: enum.Type.html
    /// [`Variable`]: type.Variable.html
    pub fn substitutions(&self) -> &HashMap<Variable, Type> {
        &self.substitutions
    }
    /// Create a new substitution for [`Type::Variable`] number `v` to the
    /// [`Type`] `t`.
    ///
    /// [`Type`]: enum.Type.html
    /// [`Type::Variable`]: enum.Type.html#variant.Variable
    pub fn extend(&mut self, v: Variable, t: Type) {
        self.substitutions.insert(v, t);
    }
    /// Create a new [`Type::Variable`] from the next unused number.
    ///
    /// [`Type::Variable`]: enum.Type.html#variant.Variable
    pub fn new_variable(&mut self) -> Type {
        self.next += 1;
        Type::Variable(self.next - 1)
    }
    /// Create a new substitution from the next unused [`Type::Variable`] to the
    /// [`Type`] `t`
    ///
    /// [`Type`]: enum.Type.html
    /// [`Type::Variable`]: enum.Type.html#variant.Variable
    pub fn extend_fresh(&mut self, t: Type) {
        if let Type::Variable(v) = self.new_variable() {
            self.extend(v, t);
        }
    }
    /// Create constraints within the context that ensure `t1` and `t2`
    /// unify.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Context;
    /// let mut ctx = Context::default();
    ///
    /// let t1 = arrow![tp!(int), tp!(0)];
    /// let t2 = arrow![tp!(1), tp!(bool)];
    /// ctx.unify(&t1, &t2).expect("unifies");
    ///
    /// let t1 = t1.apply(&ctx);
    /// let t2 = t2.apply(&ctx);
    /// assert_eq!(t1, t2);
    /// # }
    /// ```
    ///
    /// Unification errors leave the context unaffected. A
    /// [`UnificationError::Failure`] error happens when symbols don't match:
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::{Context, UnificationError};
    /// let mut ctx = Context::default();
    ///
    /// let t1 = arrow![tp!(int), tp!(0)];
    /// let t2 = arrow![tp!(bool), tp!(1)];
    /// let res = ctx.unify(&t1, &t2);
    ///
    /// if let Err(UnificationError::Failure(left, right)) = res {
    ///     // failed to unify t1 with t2.
    ///     assert_eq!(left, tp!(int));
    ///     assert_eq!(right, tp!(bool));
    /// } else { unreachable!() }
    /// # }
    /// ```
    ///
    /// An [`UnificationError::Occurs`] error happens when the same type
    /// variable occurs in both types in a circular way. Ensure you
    /// [`instantiate`][] your types properly, so type variables don't overlap
    /// unless you mean them to.
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::{Context, UnificationError};
    /// let mut ctx = Context::default();
    ///
    /// let t1 = tp!(1);
    /// let t2 = arrow![tp!(bool), tp!(1)];
    /// let res = ctx.unify(&t1, &t2);
    ///
    /// if let Err(UnificationError::Occurs(v)) = res {
    ///     // failed to unify t1 with t2 because of circular type variable occurrence.
    ///     // t1 would have to be bool -> bool -> ... ad infinitum.
    ///     assert_eq!(v, 1);
    /// } else { unreachable!() }
    /// # }
    /// ```
    ///
    /// [`UnificationError::Failure`]: enum.UnificationError.html#variant.Failure
    /// [`UnificationError::Occurs`]: enum.UnificationError.html#variant.Occurs
    /// [`instantiate`]: enum.Type.html#method.instantiate
    pub fn unify(&mut self, t1: &Type, t2: &Type) -> Result<(), UnificationError> {
        let mut ctx = self.clone();
        ctx.unify_internal(t1, t2)?;
        *self = ctx;
        Ok(())
    }
    /// unify_internal may mutate the context even with an error. The context on
    /// which it's called should be discarded if there's an error.
    fn unify_internal(&mut self, t1: &Type, t2: &Type) -> Result<(), UnificationError> {
        let t1 = t1.apply(self);
        let t2 = t2.apply(self);
        if t1 == t2 {
            return Ok(());
        }
        match (t1, t2) {
            (Type::Variable(v), t2) => {
                if t2.occurs(v) {
                    Err(UnificationError::Occurs(v))
                } else {
                    self.extend(v, t2.clone());
                    Ok(())
                }
            }
            (t1, Type::Variable(v)) => {
                if t1.occurs(v) {
                    Err(UnificationError::Occurs(v))
                } else {
                    self.extend(v, t1.clone());
                    Ok(())
                }
            }
            (Type::Constructed(n1, a1), Type::Constructed(n2, a2)) => {
                if n1 != n2 {
                    Err(UnificationError::Failure(
                        Type::Constructed(n1, a1),
                        Type::Constructed(n2, a2),
                    ))
                } else {
                    for (t1, t2) in a1.into_iter().zip(a2) {
                        self.unify_internal(&t1, &t2)?;
                    }
                    Ok(())
                }
            }
        }
    }
}

mod parser {
    use std::num::ParseIntError;
    use nom::types::CompleteStr;
    use nom::{alpha, digit};

    use super::{Polytype, Type};

    fn nom_u32(inp: CompleteStr) -> Result<u32, ParseIntError> {
        inp.0.parse()
    }

    named!(var<CompleteStr, Type>,
           do_parse!(tag!("t") >>
                     num: map_res!(digit, nom_u32) >>
                     (Type::Variable(num)))
    );
    named!(constructed_simple<CompleteStr, Type>,
           do_parse!(
               name: alpha >>
                   (Type::Constructed(leaky_str(name.0), vec![])))
    );
    named!(constructed_complex<CompleteStr, Type>,
           do_parse!(
               name: alpha >>
                   args: delimited!(
                       tag!("("),
                       separated_list!(tag!(","), ws!(monotype)),
                       tag!(")")
                   ) >>
                   (Type::Constructed(leaky_str(name.0), args)))
    );
    named!(arrow<CompleteStr, Type>,
           do_parse!(alpha: ws!(alt!(parenthetical |
                                     var |
                                     constructed_complex |
                                     constructed_simple)) >>
                     alt!(tag!("→") | tag!("->")) >>
                     beta: ws!(monotype) >>
                     (Type::arrow(alpha, beta)))
    );
    named!(parenthetical<CompleteStr, Type>,
           delimited!(tag!("("), arrow, tag!(")"))
    );
    named!(binding<CompleteStr, Polytype>,
           do_parse!(tag!("t") >>
                     variable: map_res!(digit, nom_u32) >>
                     ws!(tag!(".")) >>
                     body: map!(polytype, |p| Box::new(p)) >>
                     (Polytype::Binding{variable, body}))
    );
    named!(monotype<CompleteStr, Type>,
           alt!(arrow | var | constructed_complex | constructed_simple)
    );
    named!(polytype<CompleteStr, Polytype>,
           alt!(map!(monotype, |t| Polytype::Monotype(t)) | binding)
    );

    pub fn parse(input: &str) -> Result<Type, ()> {
        parsem(input)
    }
    pub fn parsem(input: &str) -> Result<Type, ()> {
        match monotype(CompleteStr(input)) {
            Ok((_, t)) => Ok(t),
            _ => Err(()),
        }
    }
    pub fn parsep(input: &str) -> Result<Polytype, ()> {
        match polytype(CompleteStr(input)) {
            Ok((_, t)) => Ok(t),
            _ => Err(()),
        }
    }

    fn leaky_str(s: &str) -> &'static str {
        unsafe { &mut *Box::into_raw(s.to_string().into_boxed_str()) }
    }
}
