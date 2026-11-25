//! `$math(...)$`: a common subset of mXparser, which Streamer.bot uses. Numbers, `+ - * / ^`, `#`
//! (modulo), postfix `%` (percent) and `!` (factorial), comparisons (`= == <> != ~= < > <= >=`, giving
//! 1 or 0), `& && | || ~` (not), parentheses, constants `pi` and `e`, and the functions below.
//! Anything else is NaN, as in Streamer.bot.

/// Maximum nesting depth for parentheses, unary chains, exponents and function calls.
const MAX_DEPTH: usize = 64;

/// The value of an expression; NaN when it can't be read.
pub fn eval(s: &str) -> f64 {
    let mut p = Parser {
        s: s.as_bytes(),
        i: 0,
        depth: 0,
    };
    match p.or() {
        Some(v) if p.at_end() => v,
        _ => f64::NAN,
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

fn truth(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

impl Parser<'_> {
    fn enter(&mut self) -> Option<()> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        Some(())
    }

    fn skip(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn at_end(&mut self) -> bool {
        self.skip();
        self.i == self.s.len()
    }

    /// Take `tok` if it's next.
    fn eat(&mut self, tok: &str) -> bool {
        self.skip();
        if self.s[self.i..].starts_with(tok.as_bytes()) {
            self.i += tok.len();
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Option<f64> {
        let mut v = self.and()?;
        while self.eat("||") || self.eat("|") {
            let r = self.and()?;
            v = if v.is_nan() || r.is_nan() {
                f64::NAN
            } else {
                truth(v != 0.0 || r != 0.0)
            };
        }
        Some(v)
    }

    fn and(&mut self) -> Option<f64> {
        let mut v = self.cmp()?;
        while self.eat("&&") || self.eat("&") {
            let r = self.cmp()?;
            v = if v.is_nan() || r.is_nan() {
                f64::NAN
            } else {
                truth(v != 0.0 && r != 0.0)
            };
        }
        Some(v)
    }

    fn cmp(&mut self) -> Option<f64> {
        let mut v = self.sum()?;
        loop {
            // Longest operators first.
            let op = ["==", "<>", "!=", "~=", "<=", ">=", "=", "<", ">"]
                .into_iter()
                .find(|op| self.eat(op));
            let Some(op) = op else { return Some(v) };
            let r = self.sum()?;
            v = if v.is_nan() || r.is_nan() {
                f64::NAN
            } else {
                truth(match op {
                    "=" | "==" => v == r,
                    "<>" | "!=" | "~=" => v != r,
                    "<" => v < r,
                    ">" => v > r,
                    "<=" => v <= r,
                    _ => v >= r,
                })
            };
        }
    }

    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        loop {
            if self.eat("+") {
                v += self.product()?;
            } else if self.eat("-") {
                v -= self.product()?;
            } else {
                return Some(v);
            }
        }
    }

    fn product(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            if self.eat("*") {
                v *= self.unary()?;
            } else if self.eat("/") {
                v /= self.unary()?;
            } else if self.eat("#") {
                v %= self.unary()?;
            } else {
                return Some(v);
            }
        }
    }

    /// Unary minus/plus/not bind looser than ^: -2^2 is -4.
    fn unary(&mut self) -> Option<f64> {
        if self.eat("-") {
            self.enter()?;
            let v = self.unary();
            self.depth -= 1;
            return Some(-v?);
        }
        if self.eat("+") {
            self.enter()?;
            let v = self.unary();
            self.depth -= 1;
            return v;
        }
        if self.eat("~") {
            self.enter()?;
            let v = self.unary();
            self.depth -= 1;
            let v = v?;
            return Some(if v.is_nan() { f64::NAN } else { truth(v == 0.0) });
        }
        self.power()
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if self.eat("^") {
            // Right-associative, and the exponent may be negative: 2^-1.
            self.enter()?;
            let exp = self.unary();
            self.depth -= 1;
            return Some(base.powf(exp?));
        }
        Some(base)
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut v = self.atom()?;
        loop {
            // "!=" is a comparison, not a factorial.
            self.skip();
            if self.s[self.i..].starts_with(b"!=") {
                return Some(v);
            }
            if self.eat("%") {
                v /= 100.0;
            } else if self.eat("!") {
                v = if v >= 0.0 && v.fract() == 0.0 && v <= 170.0 {
                    (1..=v as u32).map(f64::from).product()
                } else {
                    f64::NAN
                };
            } else {
                return Some(v);
            }
        }
    }

    fn atom(&mut self) -> Option<f64> {
        self.skip();
        if self.eat("(") {
            self.enter()?;
            let v = self.or();
            self.depth -= 1;
            let v = v?;
            return self.eat(")").then_some(v);
        }
        let start = self.i;
        let c = *self.s.get(self.i)?;
        if c.is_ascii_digit() || c == b'.' {
            while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
                self.i += 1;
            }
            // An exponent: 1e20, 2.5E-3.
            if self.s.get(self.i).is_some_and(|c| *c == b'e' || *c == b'E') {
                let save = self.i;
                self.i += 1;
                if self.s.get(self.i).is_some_and(|c| *c == b'+' || *c == b'-') {
                    self.i += 1;
                }
                if self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
                    while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
                        self.i += 1;
                    }
                } else {
                    self.i = save;
                }
            }
            return std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok();
        }
        if !c.is_ascii_alphabetic() {
            return None;
        }
        while self
            .s
            .get(self.i)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.i += 1;
        }
        let name = std::str::from_utf8(&self.s[start..self.i]).ok()?.to_ascii_lowercase();
        match name.as_str() {
            "pi" => return Some(std::f64::consts::PI),
            "e" => return Some(std::f64::consts::E),
            "rand" | "rnd" if !self.peek_paren() => return Some(random()),
            _ => {}
        }
        if !self.eat("(") {
            return None;
        }
        self.enter()?;
        let mut args = Vec::new();
        let ok = if self.eat(")") {
            true
        } else {
            loop {
                let Some(arg) = self.or() else { break false };
                args.push(arg);
                if self.eat(")") {
                    break true;
                }
                if !self.eat(",") {
                    break false;
                }
            }
        };
        self.depth -= 1;
        if !ok {
            return None;
        }
        call(&name, &args)
    }

    fn peek_paren(&mut self) -> bool {
        self.skip();
        self.s.get(self.i) == Some(&b'(')
    }
}

/// 64 unpredictable bits (for auth salts and ids): SipHash keyed by the OS's random keys (std's
/// RandomState), over a counter and the clock.
pub fn secure_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let mut h = std::hash::RandomState::new().build_hasher();
    h.write_u64(N.fetch_add(1, Ordering::Relaxed));
    h.write_u128(crate::fmt::unix_now().as_nanos());
    h.finish()
}

/// A random number in [0, 1). ponytail: xorshift seeded from the clock; fine for chat games.
pub fn random() -> f64 {
    use std::cell::Cell;
    thread_local!(static S: Cell<u64> = const { Cell::new(0) });
    S.with(|s| {
        let mut x = s.get();
        if x == 0 {
            x = crate::fmt::unix_now().as_nanos() as u64 | 1;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn call(name: &str, a: &[f64]) -> Option<f64> {
    let one = |f: fn(f64) -> f64| (a.len() == 1).then(|| f(a[0]));
    match name {
        "abs" => one(f64::abs),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "sqrt" => one(f64::sqrt),
        "exp" => one(f64::exp),
        "ln" => one(f64::ln),
        "log10" | "lg" => one(f64::log10),
        "log2" => one(f64::log2),
        "sin" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" | "tg" => one(f64::tan),
        "asin" | "arcsin" => one(f64::asin),
        "acos" | "arccos" => one(f64::acos),
        "atan" | "arctan" | "arctg" => one(f64::atan),
        "sgn" | "sign" => one(|x| if x == 0.0 { 0.0 } else { x.signum() }),
        // mXparser's log(b, x) is log base b of x.
        "log" => (a.len() == 2).then(|| a[1].ln() / a[0].ln()),
        // round(x) or round(x, decimals), half away from zero.
        "round" => match a {
            [x] => Some(x.round()),
            [x, n] => {
                if x.is_nan() || !n.is_finite() {
                    return Some(f64::NAN);
                }
                if !x.is_finite() || *n > 324.0 {
                    return Some(*x);
                }
                if *n < -308.0 {
                    return Some(0.0);
                }
                let p = 10f64.powi(*n as i32);
                if p == 0.0 {
                    Some(0.0)
                } else if !p.is_finite() || !(x * p).is_finite() {
                    Some(*x)
                } else {
                    Some((x * p).round() / p)
                }
            }
            _ => None,
        },
        "min" => (!a.is_empty()).then(|| {
            if a.iter().any(|x| x.is_nan()) {
                f64::NAN
            } else {
                a.iter().copied().fold(f64::INFINITY, f64::min)
            }
        }),
        "max" => (!a.is_empty()).then(|| {
            if a.iter().any(|x| x.is_nan()) {
                f64::NAN
            } else {
                a.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            }
        }),
        "mod" => (a.len() == 2).then(|| a[0] % a[1]),
        "rand" | "rnd" => match a {
            [] => Some(random()),
            // rand(a, b): an integer between a and b, inclusive.
            [lo, hi] => {
                if !lo.is_finite() || !hi.is_finite() || lo > hi {
                    Some(f64::NAN)
                } else {
                    Some((lo + (random() * (hi - lo + 1.0)).floor()).min(*hi))
                }
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn like_streamerbot() {
        assert_eq!(eval("10+10"), 20.0);
        assert_eq!(eval("10 * 1.25"), 12.5);
        assert_eq!(eval("7/2"), 3.5);
        assert_eq!(eval("2^10"), 1024.0);
        assert!(eval("10 % 3").is_nan());
        assert_eq!(eval("50%"), 0.5);
        assert_eq!(eval("10#3"), 1.0);
        assert_eq!(eval("round(2.567, 2)"), 2.57);
        assert_eq!(eval("1=1"), 1.0);
        assert_eq!(eval("5 > 3"), 1.0);
        assert!(eval("abc").is_nan());
        assert_eq!(eval("-3"), -3.0);
        assert_eq!(eval("-2^2"), -4.0);
        assert_eq!(eval("2^-1"), 0.5);
        assert_eq!(eval("1e20"), 1e20);
        assert_eq!(eval("max(1,5,3)"), 5.0);
        assert_eq!(eval("floor(-2.5)"), -3.0);
        assert_eq!(eval("2*pi"), 2.0 * std::f64::consts::PI);
        assert_eq!(eval("(1 < 2) & ~(3 <> 3)"), 1.0);
        assert_eq!(eval("1 != 2"), 1.0);
        assert_eq!(eval("4!"), 24.0);
        assert_eq!(eval("log(2, 8)"), 3.0);
        assert!(eval("1 +").is_nan());
        let r = eval("rand(1, 6)");
        assert!((1.0..=6.0).contains(&r) && r.fract() == 0.0);
    }

    #[test]
    fn deep_nesting_returns_nan_without_overflowing_stack() {
        let parens = format!("{}{}", "(".repeat(2_000), ")".repeat(2_000));
        assert!(eval(&parens).is_nan());
        assert!(eval(&format!("{}1", "-".repeat(2_000))).is_nan());
        assert!(eval(&format!("{}1", "2^".repeat(2_000))).is_nan());
        let nested_fn = format!("{}1{}", "abs(".repeat(2_000), ")".repeat(2_000));
        assert!(eval(&nested_fn).is_nan());
    }

    #[test]
    fn nan_and_infinity_propagate_cleanly() {
        assert!(eval("sqrt(-1) || 0").is_nan());
        assert!(eval("sqrt(-1) && 1").is_nan());
        assert!(eval("~sqrt(-1)").is_nan());
        assert!(eval("sqrt(-1) != 1").is_nan());
        assert!(eval("sqrt(-1) == sqrt(-1)").is_nan());
        assert!(eval("min(sqrt(-1))").is_nan());
        assert!(eval("max(sqrt(-1))").is_nan());
        assert!(eval("min(1, sqrt(-1), 2)").is_nan());
        assert!(eval("max(1, sqrt(-1), 2)").is_nan());
        assert_eq!(eval("1 / 0"), f64::INFINITY);
        assert_eq!(eval("-1 / 0"), f64::NEG_INFINITY);
        assert!(eval("0 / 0").is_nan());
        assert!(eval("10 # 0").is_nan());
        assert_eq!(eval("2^10000"), f64::INFINITY);
        assert_eq!(eval("round(1.5, 400)"), 1.5);
        assert_eq!(eval("round(1.5, -400)"), 0.0);
        assert_eq!(eval("round(1e300, 20)"), 1e300);
        assert!(eval("round(1.5, sqrt(-1))").is_nan());
        assert!(eval("rand(6, 1)").is_nan());
        assert!(eval("rand(1, 1/0)").is_nan());
    }

    #[test]
    fn precedence_functions_and_edge_cases() {
        assert_eq!(eval("2^3^2"), 512.0);
        assert_eq!(eval("(-2)^2"), 4.0);
        assert_eq!(eval("2^-2^2"), 0.0625);
        assert_eq!(eval("0!"), 1.0);
        assert!(eval("170!").is_finite());
        assert!(eval("171!").is_nan());
        assert!(eval("(-1)!").is_nan());
        assert!(eval("1.5!").is_nan());
        assert_eq!(eval("2^3!"), 64.0);
        assert_eq!(eval("3!^2"), 36.0);
        assert_eq!(eval("2.5e-3"), 0.0025);
        assert_eq!(eval(".5 + .5"), 1.0);
        assert!(eval(".").is_nan());
        assert!(eval("1.2.3").is_nan());
        assert!(eval("1e+").is_nan());
        assert_eq!(eval("abs(-5) + ceil(1.2) + sqrt(9) + mod(10, 3)"), 11.0);
        assert_eq!(eval("sgn(-4)"), -1.0);
        assert_eq!(eval("sgn(0)"), 0.0);
        assert_eq!(eval("sgn(7)"), 1.0);
        assert!(eval("sgn(sqrt(-1))").is_nan());
        assert_eq!(eval("log10(1000) + log2(8)"), 6.0);
        assert_eq!(eval("exp(0) + ln(1)"), 1.0);
        assert!((eval("sin(0) + cos(0) + tan(0)") - 1.0).abs() < 1e-12);
        assert_eq!(eval("2 <= 2 && 3 >= 2 && 1 ~= 2"), 1.0);
        assert_eq!(eval("0 || 0"), 0.0);
        assert_eq!(eval("0 || 2"), 1.0);
        assert!((0.0..1.0).contains(&eval("rand")));
        assert!((0.0..1.0).contains(&eval("rnd()")));
        assert!(eval("").is_nan());
        assert!(eval("abs()").is_nan());
        assert!(eval("round(1, 2, 3)").is_nan());
    }
}
