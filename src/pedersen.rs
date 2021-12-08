use bitvec::{array::BitArray, order::Lsb0, slice::BitSlice};
use ff::{Field, PrimeField};

/// The field primitive used by [PedersenHash]
#[derive(PrimeField)]
#[PrimeFieldModulus = "3618502788666131213697322783095070105623107215331596699973092056135872020481"]
#[PrimeFieldGenerator = "7"]
#[PrimeFieldReprEndianness = "little"]
pub struct Fp([u64; 4]);

impl Fp {
    /// Transforms [Fp] into little endian bit representation.
    fn into_bits(mut self) -> BitArray<Lsb0, [u64; 4]> {
        #[cfg(not(target_endian = "little"))]
        {
            todo!("untested and probably unimplemented: big-endian targets")
        }

        #[cfg(target_endian = "little")]
        {
            self.mont_reduce(
                self.0[0usize],
                self.0[1usize],
                self.0[2usize],
                self.0[3usize],
                0,
                0,
                0,
                0,
            );

            self.0.into()
        }
    }
}

/// A point on an elliptic curve over [Fp].
#[derive(Clone, Debug, Eq, PartialEq)]
struct CurvePoint {
    x: Fp,
    y: Fp,
    infinity: bool,
}

impl CurvePoint {
    fn identity() -> CurvePoint {
        Self {
            x: Fp::zero(),
            y: Fp::zero(),
            infinity: true,
        }
    }

    fn double(&self) -> CurvePoint {
        if self.infinity {
            return self.clone();
        }

        // l = (3x^2+a)/2y with a=1 from stark curve
        let lambda = {
            let two = Fp::one() + Fp::one();
            let three = two + Fp::one();
            let dividend = three * (self.x * self.x) + Fp::one();
            let divisor_inv = (two * self.y).invert().unwrap();
            dividend * divisor_inv
        };

        let result_x = (lambda * lambda) - self.x - self.x;
        let result_y = lambda * (self.x - result_x) - self.y;

        CurvePoint {
            x: result_x,
            y: result_y,
            infinity: false,
        }
    }

    fn add(&self, other: &CurvePoint) -> CurvePoint {
        if self.infinity {
            return other.clone();
        }
        if other.infinity {
            return self.clone();
        }

        // l = (y2-y1)/(x2-x1)
        let lambda = {
            let dividend = other.y - self.y;
            let divisor_inv = (other.x - self.x).invert().unwrap();
            dividend * divisor_inv
        };

        let result_x = (lambda * lambda) - self.x - other.x;
        let result_y = lambda * (self.x - result_x) - self.y;

        CurvePoint {
            x: result_x,
            y: result_y,
            infinity: false,
        }
    }

    fn multiply(&self, bits: &BitSlice<Lsb0, u64>) -> CurvePoint {
        let mut product = CurvePoint::identity();
        for b in bits.iter().rev() {
            product = product.double();
            if *b {
                product = product.add(self);
            }
        }

        product
    }
}

/// Montgomery representation of the Stark curve constant P0.
const PEDERSEN_P0: CurvePoint = CurvePoint {
    x: Fp([
        1933903796324928314,
        7739989395386261137,
        1641324389046377921,
        316327189671755572,
    ]),
    y: Fp([
        14252083571674603243,
        12587053260418384210,
        4798858472748676776,
        81375596133053150,
    ]),
    infinity: false,
};

/// Montgomery representation of the Stark curve constant P1.
const PEDERSEN_P1: CurvePoint = CurvePoint {
    x: Fp([
        3602345268353203007,
        13758484295849329960,
        518715844721862878,
        241691544791834578,
    ]),
    y: Fp([
        13441546676070136227,
        13001553326386915570,
        433857700841878496,
        368891789801938570,
    ]),
    infinity: false,
};

/// Montgomery representation of the Stark curve constant P2.
const PEDERSEN_P2: CurvePoint = CurvePoint {
    x: Fp([
        16491878934996302286,
        12382025591154462459,
        10043949394709899044,
        253000153565733272,
    ]),
    y: Fp([
        13950428914333633429,
        2545498000137298346,
        5191292837124484988,
        285630633187035523,
    ]),
    infinity: false,
};

/// Montgomery representation of the Stark curve constant P3.
const PEDERSEN_P3: CurvePoint = CurvePoint {
    x: Fp([
        1203723169299412240,
        18195981508842736832,
        12916675983929588442,
        338510149841406402,
    ]),
    y: Fp([
        12352616181161700245,
        11743524503750604092,
        11088962269971685343,
        161068411212710156,
    ]),
    infinity: false,
};

/// Montgomery representation of the Stark curve constant P4.
const PEDERSEN_P4: CurvePoint = CurvePoint {
    x: Fp([
        1145636535101238356,
        10664803185694787051,
        299781701614706065,
        425493972656615276,
    ]),
    y: Fp([
        8187986478389849302,
        4428713245976508844,
        6033691581221864148,
        345457391846365716,
    ]),
    infinity: false,
};

/// Performs the Stark Pedersen hash on `a` and `b`.
pub fn pedersen_hash(a: Fp, b: Fp) -> Fp {
    let mut result = PEDERSEN_P0.clone();
    let a = a.into_bits();
    let b = b.into_bits();

    // Add a_low * P1
    let tmp = PEDERSEN_P1.multiply(&a[..248]);
    result = result.add(&tmp);

    // Add a_high * P2
    let tmp = PEDERSEN_P2.multiply(&a[248..252]);
    result = result.add(&tmp);

    // Add b_low * P3
    let tmp = PEDERSEN_P3.multiply(&b[..248]);
    result = result.add(&tmp);

    // Add b_high * P4
    let tmp = PEDERSEN_P4.multiply(&b[248..252]);
    result = result.add(&tmp);

    // Return x-coordinate
    result.x
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    mod to_le_bits_rev {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn zero() {
            let zero = Fp::zero().into_bits();
            let expected = BitArray::<Lsb0, [u64; 4]>::default();

            assert_eq!(zero, expected);
        }

        #[test]
        fn one() {
            let one = Fp::one().into_bits();

            let mut expected = BitArray::<Lsb0, [u64; 4]>::default();
            expected.set(0, true);

            assert_eq!(one, expected);
        }

        #[test]
        fn two() {
            let two = (Fp::one() + Fp::one()).into_bits();

            let mut expected = BitArray::<Lsb0, [u64; 4]>::default();
            expected.set(1, true);

            assert_eq!(two, expected);
        }
    }

    mod curve {
        use super::*;
        use pretty_assertions::assert_eq;

        fn curve_from_xy_str(x: &str, y: &str) -> CurvePoint {
            let x = Fp::from_str_vartime(x).expect("Curve x-value invalid");
            let y = Fp::from_str_vartime(y).expect("Curve y-value invalid");
            CurvePoint {
                x,
                y,
                infinity: false,
            }
        }

        fn curve_generator() -> CurvePoint {
            curve_from_xy_str(
                "874739451078007766457464989774322083649278607533249481151382481072868806602",
                "152666792071518830868575557812948353041420400780739481342941381225525861407",
            )
        }

        #[test]
        fn double() {
            let g_double = curve_generator().double();
            let expected = curve_from_xy_str(
                "3324833730090626974525872402899302150520188025637965566623476530814354734325",
                "3147007486456030910661996439995670279305852583596209647900952752170983517249",
            );
            assert_eq!(g_double, expected);
        }

        #[test]
        fn double_and_add() {
            let g = curve_generator();
            let g_double = g.double();
            let g_triple = g_double.add(&g);
            let expected = curve_from_xy_str(
                "1839793652349538280924927302501143912227271479439798783640887258675143576352",
                "3564972295958783757568195431080951091358810058262272733141798511604612925062",
            );
            assert_eq!(g_triple, expected);
        }

        #[test]
        fn multiply() {
            let three = (Fp::one() + Fp::one() + Fp::one()).into_bits();
            let g = curve_generator();
            let g_triple = g.multiply(&three);
            let expected = curve_from_xy_str(
                "1839793652349538280924927302501143912227271479439798783640887258675143576352",
                "3564972295958783757568195431080951091358810058262272733141798511604612925062",
            );
            assert_eq!(g_triple, expected);
        }

        #[test]
        fn p0() {
            let expected = curve_from_xy_str(
                "2089986280348253421170679821480865132823066470938446095505822317253594081284",
                "1713931329540660377023406109199410414810705867260802078187082345529207694986",
            );

            assert_eq!(PEDERSEN_P0, expected);
        }

        #[test]
        fn p1() {
            let expected = curve_from_xy_str(
                "996781205833008774514500082376783249102396023663454813447423147977397232763",
                "1668503676786377725805489344771023921079126552019160156920634619255970485781",
            );

            assert_eq!(PEDERSEN_P1, expected);
        }

        #[test]
        fn p2() {
            let expected = curve_from_xy_str(
                "2251563274489750535117886426533222435294046428347329203627021249169616184184",
                "1798716007562728905295480679789526322175868328062420237419143593021674992973",
            );

            assert_eq!(PEDERSEN_P2, expected);
        }

        #[test]
        fn p3() {
            let expected = curve_from_xy_str(
                "2138414695194151160943305727036575959195309218611738193261179310511854807447",
                "113410276730064486255102093846540133784865286929052426931474106396135072156",
            );

            assert_eq!(PEDERSEN_P3, expected);
        }

        #[test]
        fn p4() {
            let expected = curve_from_xy_str(
                "2379962749567351885752724891227938183011949129833673362440656643086021394946",
                "776496453633298175483985398648758586525933812536653089401905292063708816422",
            );

            assert_eq!(PEDERSEN_P4, expected);
        }
    }

    #[test]
    fn hash() {
        // Test vector from https://github.com/starkware-libs/crypto-cpp/blob/master/src/starkware/crypto/pedersen_hash_test.cc
        let a = Fp::from_str_vartime(
            "1740729136829561885683894917751815192814966525555656371386868611731128807883",
        )
        .unwrap();
        let b = Fp::from_str_vartime(
            "919869093895560023824014392670608914007817594969197822578496829435657368346",
        )
        .unwrap();

        let hash = pedersen_hash(a, b);

        let expected = Fp::from_str_vartime(
            "1382171651951541052082654537810074813456022260470662576358627909045455537762",
        )
        .unwrap();

        assert_eq!(hash, expected);
    }
}
