pub trait Zero: PartialEq + Sized {
    const ZERO: Self;

    fn is_zero(&self) -> bool;
}

impl Zero for f32 {
    const ZERO: f32 = 0.;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl Zero for f64 {
    const ZERO: f64 = 0.;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl Zero for usize {
    const ZERO: usize = 0;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl Zero for i32 {
    const ZERO: i32 = 0;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl Zero for u32 {
    const ZERO: u32 = 0;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl Zero for u64 {
    const ZERO: u64 = 0;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl Zero for i64 {
    const ZERO: i64 = 0;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

////////////////////////////////////////////////////////////////////////////////

pub trait One: Sized {
    const ONE: Self;

    fn is_one(&self) -> bool;
}

impl One for f32 {
    const ONE: f32 = 1.;

    fn is_one(&self) -> bool {
        self == &Self::ONE
    }
}

impl One for f64 {
    const ONE: f64 = 1.;

    fn is_one(&self) -> bool {
        self == &Self::ONE
    }
}

impl One for usize {
    const ONE: usize = 1;

    fn is_one(&self) -> bool {
        self == &Self::ONE
    }
}

////////////////////////////////////////////////////////////////////////////////

pub trait Two {
    const TWO: Self;
}

impl Two for f32 {
    const TWO: f32 = 2.;
}

impl Two for f64 {
    const TWO: f64 = 2.;
}

impl Two for i32 {
    const TWO: i32 = 2;
}
