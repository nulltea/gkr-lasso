pub use halo2_curves::ff::{Field, PrimeField};
pub use itertools::{chain, izip, Itertools};
pub use parallel::{chunk_info, num_threads, parallelize, parallelize_iter};
pub use rand::RngCore;

pub fn div_ceil(dividend: usize, divisor: usize) -> usize {
    (dividend + divisor - 1) / divisor
}

pub fn horner<F: Field>(vs: &[F], x: &F) -> F {
    vs.iter().rev().fold(F::ZERO, |acc, v| acc * x + v)
}

pub fn inner_product<F: Field>(lhs: &[F], rhs: &[F]) -> F {
    izip_eq!(lhs, rhs).map(|(lhs, rhs)| *lhs * rhs).sum()
}

macro_rules! izip_eq {
    (@closure $p:pat => $tup:expr) => {
        |$p| $tup
    };
    (@closure $p:pat => ($($tup:tt)*) , $_iter:expr $(, $tail:expr)*) => {
        $crate::util::izip_eq!(@closure ($p, b) => ($($tup)*, b) $(, $tail)*)
    };
    ($first:expr $(,)*) => {
        itertools::__std_iter::IntoIterator::into_iter($first)
    };
    ($first:expr, $second:expr $(,)*) => {
        $crate::util::izip_eq!($first).zip_eq($second)
    };
    ($first:expr $(, $rest:expr)* $(,)*) => {
        $crate::util::izip_eq!($first)
            $(.zip_eq($rest))*
            .map($crate::util::izip_eq!(@closure a => (a) $(, $rest)*))
    };
}

pub(crate) use izip_eq;

mod parallel {
    use crate::util::div_ceil;

    pub fn num_threads() -> usize {
        #[cfg(feature = "parallel")]
        return rayon::current_num_threads();

        #[cfg(not(feature = "parallel"))]
        return 1;
    }

    pub fn chunk_info(num_tasks: usize) -> (usize, usize) {
        let chunk_size = div_ceil(num_tasks, num_threads().min(num_tasks));
        let num_chunks = div_ceil(num_tasks, chunk_size);
        (chunk_size, num_chunks)
    }

    pub fn parallelize_iter<I, T, F>(iter: I, f: F)
    where
        I: Send + Iterator<Item = T>,
        T: Send,
        F: Fn(T) + Send + Sync + Clone,
    {
        #[cfg(feature = "parallel")]
        rayon::scope(|scope| {
            iter.for_each(|item| {
                let f = &f;
                scope.spawn(move |_| f(item))
            })
        });

        #[cfg(not(feature = "parallel"))]
        iter.for_each(f);
    }

    pub fn parallelize<T, F>(v: &mut [T], f: F)
    where
        T: Send,
        F: Fn((&mut [T], usize)) + Send + Sync + Clone,
    {
        #[cfg(feature = "parallel")]
        {
            let num_tasks = v.len();
            let num_threads = num_threads();
            let chunk_size_lo = num_tasks / num_threads;
            let chunk_size_hi = chunk_size_lo + 1;
            let mid = (num_tasks % num_threads) * chunk_size_hi;
            let (v_hi, v_lo) = v.split_at_mut(mid);
            let f = &f;

            rayon::scope(|scope| {
                if chunk_size_hi > 0 {
                    for (idx, v) in v_hi.chunks_exact_mut(chunk_size_hi).enumerate() {
                        scope.spawn(move |_| f((v, idx * chunk_size_hi)));
                    }
                }
                if chunk_size_lo > 0 {
                    for (idx, v) in v_lo.chunks_exact_mut(chunk_size_lo).enumerate() {
                        scope.spawn(move |_| f((v, mid + idx * chunk_size_lo)));
                    }
                }
            });
        }

        #[cfg(not(feature = "parallel"))]
        f((v, 0));
    }
}

#[cfg(test)]
pub mod test {
    use crate::util::Field;
    use rand::{
        distributions::uniform::SampleRange,
        rngs::{OsRng, StdRng},
        CryptoRng, Rng, RngCore, SeedableRng,
    };
    use std::{array, iter};

    pub fn std_rng() -> impl RngCore + CryptoRng {
        StdRng::from_seed(Default::default())
    }

    pub fn seeded_std_rng() -> impl RngCore + CryptoRng {
        StdRng::seed_from_u64(OsRng.next_u64())
    }

    pub fn rand_range(range: impl SampleRange<usize>, mut rng: impl RngCore) -> usize {
        rng.gen_range(range)
    }

    pub fn rand_bool(mut rng: impl RngCore) -> bool {
        rng.gen_bool(0.5)
    }

    pub fn rand_array<F: Field, const N: usize>(mut rng: impl RngCore) -> [F; N] {
        array::from_fn(|_| F::random(&mut rng))
    }

    pub fn rand_vec<F: Field>(n: usize, mut rng: impl RngCore) -> Vec<F> {
        iter::repeat_with(|| F::random(&mut rng)).take(n).collect()
    }
}
