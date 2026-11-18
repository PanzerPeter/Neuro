//! Symbolic tensor extents: the `N * 2` a `.flatten()` of `[N, 2]` produces, or the
//! `D / H` written in `Tensor<f32, [B, H, D / H]>`.
//!
//! A generic body is checked once, before any extent has a value, so an extent computed
//! from parameters has to be a value the checker can compare. It is kept as a polynomial
//! with integer coefficients over atoms, in a normal form: two extents that agree by the
//! laws of `+`, `-` and `*` compare equal, so `[2 * N]` is the type `[N, 2].flatten()`
//! returns. `/` and `%` are floored integer operations with no such laws, so an
//! application that does not simplify exactly is one more atom.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A polynomial over [`Atom`]s: each monomial (atoms raised to powers) with its nonzero
/// coefficient. The empty monomial is the constant term, and no terms at all is zero.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExtentExpr {
    terms: BTreeMap<Monomial, i128>,
}

/// A product of atoms, each with its power. Empty is the constant 1.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Monomial(BTreeMap<Atom, u32>);

/// What an extent polynomial is built over.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Atom {
    /// An extent parameter, `N`.
    Param(String),
    /// A shape parameter's element count, `S.count()`: what a spliced shape contributes
    /// to a product.
    Count(String),
    /// A floored quotient that did not divide out.
    Quot(Box<ExtentExpr>, Box<ExtentExpr>),
    /// A remainder that did not fold.
    Rem(Box<ExtentExpr>, Box<ExtentExpr>),
}

impl Atom {
    /// The size parameter a parameter or count atom reads.
    pub fn name(&self) -> Option<&str> {
        match self {
            Atom::Param(name) | Atom::Count(name) => Some(name),
            Atom::Quot(..) | Atom::Rem(..) => None,
        }
    }
}

/// An extent with no value: a zero divisor, a negative operand of `/` or `%`, or a
/// coefficient past `i128`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoValue;

impl Monomial {
    fn one() -> Self {
        Monomial(BTreeMap::new())
    }

    fn times(&self, other: &Monomial) -> Monomial {
        let mut product = self.0.clone();
        for (atom, power) in &other.0 {
            *product.entry(atom.clone()).or_insert(0) += power;
        }
        Monomial(product)
    }

    /// `self / other` when `other`'s atoms all divide `self`'s.
    fn divided_by(&self, other: &Monomial) -> Option<Monomial> {
        let mut quotient = self.0.clone();
        for (atom, power) in &other.0 {
            let have = quotient.get_mut(atom)?;
            match (*have).cmp(power) {
                std::cmp::Ordering::Less => return None,
                std::cmp::Ordering::Equal => {
                    quotient.remove(atom);
                }
                std::cmp::Ordering::Greater => *have -= power,
            }
        }
        Some(Monomial(quotient))
    }

    /// The atoms both share, each at the lower power.
    fn common(&self, other: &Monomial) -> Monomial {
        Monomial(
            self.0
                .iter()
                .filter_map(|(atom, power)| Some((atom.clone(), (*power).min(*other.0.get(atom)?))))
                .collect(),
        )
    }
}

impl ExtentExpr {
    pub fn constant(value: i128) -> Self {
        let mut terms = BTreeMap::new();
        if value != 0 {
            terms.insert(Monomial::one(), value);
        }
        ExtentExpr { terms }
    }

    pub fn atom(atom: Atom) -> Self {
        let mut terms = BTreeMap::new();
        terms.insert(Monomial(BTreeMap::from([(atom, 1)])), 1);
        ExtentExpr { terms }
    }

    pub fn param(name: &str) -> Self {
        Self::atom(Atom::Param(name.to_string()))
    }

    pub fn count(name: &str) -> Self {
        Self::atom(Atom::Count(name.to_string()))
    }

    /// The value, when no atom is left.
    pub fn as_constant(&self) -> Option<i128> {
        match self.terms.len() {
            0 => Some(0),
            1 => self.terms.get(&Monomial::one()).copied(),
            _ => None,
        }
    }

    /// The parameter, when the extent is exactly one.
    pub fn as_param(&self) -> Option<&str> {
        let (monomial, 1) = self.single_term()? else {
            return None;
        };
        let mut atoms = monomial.0.iter();
        match (atoms.next(), atoms.next()) {
            (Some((Atom::Param(name), 1)), None) => Some(name),
            _ => None,
        }
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, NoValue> {
        let mut terms = self.terms.clone();
        for (monomial, coefficient) in &other.terms {
            add_term(&mut terms, monomial.clone(), *coefficient)?;
        }
        Ok(ExtentExpr { terms })
    }

    pub fn checked_sub(&self, other: &Self) -> Result<Self, NoValue> {
        self.checked_add(&other.scaled(-1)?)
    }

    pub fn checked_mul(&self, other: &Self) -> Result<Self, NoValue> {
        let mut terms = BTreeMap::new();
        for (left, a) in &self.terms {
            for (right, b) in &other.terms {
                add_term(
                    &mut terms,
                    left.times(right),
                    a.checked_mul(*b).ok_or(NoValue)?,
                )?;
            }
        }
        Ok(ExtentExpr { terms })
    }

    fn scaled(&self, by: i128) -> Result<Self, NoValue> {
        let mut terms = BTreeMap::new();
        for (monomial, coefficient) in &self.terms {
            add_term(
                &mut terms,
                monomial.clone(),
                coefficient.checked_mul(by).ok_or(NoValue)?,
            )?;
        }
        Ok(ExtentExpr { terms })
    }

    /// The single term, when there is exactly one.
    fn single_term(&self) -> Option<(&Monomial, i128)> {
        let mut terms = self.terms.iter();
        match (terms.next(), terms.next()) {
            (Some((monomial, coefficient)), None) => Some((monomial, *coefficient)),
            _ => None,
        }
    }

    /// `self` divided by a one-term divisor when every term divides exactly, which is
    /// when floored division is the polynomial's own: `(2 * N + 4) / 2` is `N + 2`.
    fn exact_quotient(&self, divisor: &Self) -> Option<Self> {
        let (by, factor) = divisor.single_term()?;
        if factor == 0 {
            return None;
        }
        let mut terms = BTreeMap::new();
        for (monomial, coefficient) in &self.terms {
            if coefficient % factor != 0 {
                return None;
            }
            add_term(&mut terms, monomial.divided_by(by)?, coefficient / factor).ok()?;
        }
        Some(ExtentExpr { terms })
    }

    /// Floored `self / divisor` over non-negative extents.
    ///
    /// A factor both sides share is cancelled (`(B * D) / (B * H)` is `D / H`), which
    /// holds wherever the divisor is nonzero, the only place the quotient has a value.
    pub fn quot(&self, divisor: &Self) -> Result<Self, NoValue> {
        if let (Some(a), Some(b)) = (self.as_constant(), divisor.as_constant()) {
            return fold_div(a, b, i128::checked_div);
        }
        if divisor.as_constant() == Some(0) {
            return Err(NoValue);
        }
        if let Some(exact) = self.exact_quotient(divisor) {
            return Ok(exact);
        }
        let reduced = match (self.single_term(), divisor.single_term()) {
            (Some((num, a)), Some((den, b))) => {
                let shared = num.common(den);
                let g = gcd(a, b);
                let reduce = |m: &Monomial, c: i128| ExtentExpr {
                    terms: BTreeMap::from([(
                        m.divided_by(&shared).unwrap_or_else(Monomial::one),
                        c / g,
                    )]),
                };
                Some((reduce(num, a), reduce(den, b)))
            }
            _ => None,
        };
        let (num, den) = reduced.unwrap_or_else(|| (self.clone(), divisor.clone()));
        if den.as_constant() == Some(1) {
            return Ok(num);
        }
        Ok(Self::atom(Atom::Quot(Box::new(num), Box::new(den))))
    }

    /// `self % divisor` over non-negative extents.
    pub fn rem(&self, divisor: &Self) -> Result<Self, NoValue> {
        if let (Some(a), Some(b)) = (self.as_constant(), divisor.as_constant()) {
            return fold_div(a, b, i128::checked_rem);
        }
        if divisor.as_constant() == Some(0) {
            return Err(NoValue);
        }
        if self.exact_quotient(divisor).is_some() {
            return Ok(Self::constant(0));
        }
        Ok(Self::atom(Atom::Rem(
            Box::new(self.clone()),
            Box::new(divisor.clone()),
        )))
    }

    /// The extent with each parameter and count `value` supplies replaced, folded again.
    pub fn substitute(&self, value: &dyn Fn(&Atom) -> Option<ExtentExpr>) -> Result<Self, NoValue> {
        let mut sum = Self::constant(0);
        for (monomial, coefficient) in &self.terms {
            let mut product = Self::constant(*coefficient);
            for (atom, power) in &monomial.0 {
                let replaced = match atom {
                    Atom::Param(_) | Atom::Count(_) => {
                        value(atom).unwrap_or_else(|| Self::atom(atom.clone()))
                    }
                    Atom::Quot(a, b) => a.substitute(value)?.quot(&b.substitute(value)?)?,
                    Atom::Rem(a, b) => a.substitute(value)?.rem(&b.substitute(value)?)?,
                };
                for _ in 0..*power {
                    product = product.checked_mul(&replaced)?;
                }
            }
            sum = sum.checked_add(&product)?;
        }
        Ok(sum)
    }

    /// Every size parameter the extent reads, as a parameter or through its count.
    pub fn names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for atom in self.terms.keys().flat_map(|m| m.0.keys()) {
            match atom {
                Atom::Param(name) | Atom::Count(name) => {
                    names.insert(name.clone());
                }
                Atom::Quot(a, b) | Atom::Rem(a, b) => {
                    names.extend(a.names());
                    names.extend(b.names());
                }
            }
        }
        names
    }
}

/// Add `coefficient * monomial` into `terms`, dropping a term that cancels to zero.
fn add_term(
    terms: &mut BTreeMap<Monomial, i128>,
    monomial: Monomial,
    coefficient: i128,
) -> Result<(), NoValue> {
    let entry = terms.entry(monomial.clone()).or_insert(0);
    *entry = entry.checked_add(coefficient).ok_or(NoValue)?;
    if *entry == 0 {
        terms.remove(&monomial);
    }
    Ok(())
}

/// A constant `/` or `%`. An extent is never negative, so a negative operand (a `N - 1`
/// at `N = 0`, folded) has no value, as a zero divisor has none.
fn fold_div(a: i128, b: i128, op: fn(i128, i128) -> Option<i128>) -> Result<ExtentExpr, NoValue> {
    if a < 0 || b <= 0 {
        return Err(NoValue);
    }
    op(a, b).map(ExtentExpr::constant).ok_or(NoValue)
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    i128::try_from(a).unwrap_or(1).max(1)
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Atom::Param(name) => write!(f, "{name}"),
            Atom::Count(name) => write!(f, "{name}.count()"),
            Atom::Quot(a, b) => write!(f, "{} / {}", Operand(a), Operand(b)),
            Atom::Rem(a, b) => write!(f, "{} % {}", Operand(a), Operand(b)),
        }
    }
}

/// An operand of `/` or `%`, parenthesized unless it is a lone name or number.
struct Operand<'a>(&'a ExtentExpr);

impl fmt::Display for Operand<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bare = self.0.as_constant().is_some() || self.0.as_param().is_some();
        if bare {
            write!(f, "{}", self.0)
        } else {
            write!(f, "({})", self.0)
        }
    }
}

impl fmt::Display for ExtentExpr {
    /// Written the way a program spells it: atoms in a fixed order, the constant last,
    /// so `N + 1` rather than `1 + N`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ordered = self
            .terms
            .iter()
            .filter(|(m, _)| !m.0.is_empty())
            .chain(self.terms.iter().filter(|(m, _)| m.0.is_empty()));
        let mut first = true;
        for (monomial, coefficient) in ordered {
            let magnitude = coefficient.unsigned_abs();
            match (first, *coefficient < 0) {
                (true, true) => write!(f, "-")?,
                (false, true) => write!(f, " - ")?,
                (false, false) => write!(f, " + ")?,
                (true, false) => {}
            }
            first = false;
            let mut factors: Vec<String> = Vec::new();
            if magnitude != 1 || monomial.0.is_empty() {
                factors.push(magnitude.to_string());
            }
            for (atom, power) in &monomial.0 {
                let shown = match atom {
                    Atom::Quot(..) | Atom::Rem(..) if monomial.0.len() > 1 || magnitude != 1 => {
                        format!("({atom})")
                    }
                    _ => atom.to_string(),
                };
                factors.extend(std::iter::repeat_n(shown, *power as usize));
            }
            write!(f, "{}", factors.join(" * "))?;
        }
        if first {
            write!(f, "0")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Atom, ExtentExpr, NoValue};

    fn n() -> ExtentExpr {
        ExtentExpr::param("N")
    }

    fn c(value: i128) -> ExtentExpr {
        ExtentExpr::constant(value)
    }

    #[test]
    fn a_product_compares_by_its_normal_form() {
        let left = n().checked_mul(&c(2)).unwrap();
        let right = c(2).checked_mul(&n()).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.to_string(), "2 * N");
        let square = n()
            .checked_add(&c(1))
            .unwrap()
            .checked_mul(&n().checked_sub(&c(1)).unwrap());
        assert_eq!(
            square.unwrap(),
            n().checked_mul(&n()).unwrap().checked_sub(&c(1)).unwrap()
        );
    }

    #[test]
    fn a_shared_factor_cancels_out_of_a_quotient() {
        let b = ExtentExpr::param("B");
        let d = ExtentExpr::param("D");
        let h = ExtentExpr::param("H");
        let total = b.checked_mul(&d).unwrap();
        let known = b.checked_mul(&h).unwrap();
        let quotient = total.quot(&known).unwrap();
        assert_eq!(quotient, d.quot(&h).unwrap());
        assert_eq!(quotient.to_string(), "D / H");
        assert_eq!(
            n().checked_mul(&c(6))
                .unwrap()
                .quot(&c(3))
                .unwrap()
                .to_string(),
            "2 * N"
        );
        assert_eq!(
            n().checked_mul(&c(2))
                .unwrap()
                .quot(&c(4))
                .unwrap()
                .to_string(),
            "N / 2"
        );
    }

    #[test]
    fn an_exact_division_leaves_no_remainder() {
        let even = n().checked_mul(&c(2)).unwrap().checked_add(&c(4)).unwrap();
        assert_eq!(even.quot(&c(2)).unwrap().to_string(), "N + 2");
        assert_eq!(even.rem(&c(2)).unwrap(), c(0));
        assert_eq!(n().rem(&c(2)).unwrap().to_string(), "N % 2");
    }

    #[test]
    fn substitution_folds_to_a_value_or_reports_none() {
        let extent = n()
            .checked_sub(&c(1))
            .unwrap()
            .quot(&ExtentExpr::param("H"))
            .unwrap();
        let at = |n_value: i128, h_value: i128| {
            extent.substitute(&|atom| match atom {
                Atom::Param(name) if name == "N" => Some(c(n_value)),
                Atom::Param(name) if name == "H" => Some(c(h_value)),
                _ => None,
            })
        };
        assert_eq!(at(9, 4), Ok(c(2)));
        assert_eq!(at(9, 0), Err(NoValue));
        assert_eq!(at(0, 4), Err(NoValue));
    }

    #[test]
    fn a_count_renders_as_its_call() {
        let flat = ExtentExpr::count("S").checked_mul(&n()).unwrap();
        assert_eq!(flat.to_string(), "N * S.count()");
        assert_eq!(flat.names().into_iter().collect::<Vec<_>>(), ["N", "S"]);
        assert_eq!(n().checked_sub(&c(1)).unwrap().to_string(), "N - 1");
    }
}
