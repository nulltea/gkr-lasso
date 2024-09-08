use std::{iter, marker::PhantomData};

use ff_ext::{ff::PrimeField, ExtensionField};
use itertools::{izip, Itertools};
use plonkish_backend::util::arithmetic::split_by_chunk_bits;

use crate::{
    poly::{box_dense_poly, BoxMultilinearPoly, MultilinearPolyTerms, PolyExpr},
    util::{
        arithmetic::{div_ceil, inner_product},
        expression::Expression,
    },
};

use super::{DecomposableTable, LassoSubtable, LookupType, SubtableIndices};

#[derive(Clone, Debug, Default)]
pub struct FullLimbSubtable<F, E, const LIMB_SIZE: usize>(PhantomData<(F, E)>);

impl<F: PrimeField, E: ExtensionField<F>, const LIMB_SIZE: usize> LassoSubtable<F, E>
    for FullLimbSubtable<F, E, LIMB_SIZE>
{
    fn materialize(&self, M: usize) -> Vec<F> {
        assert_eq!(M, 1 << LIMB_SIZE);
        (0..M).map(|x| F::from(x as u64)).collect_vec()
    }

    fn evaluate_mle(&self, point: &[E]) -> E {
        let b = point.len();
        let mut result = E::ZERO;
        for i in 0..b {
            result += point[b] * F::from(1u64 << (i));
        }
        result
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReminderSubtable<F, E, const NUM_BITS: usize, const LIMB_SIZE: usize>(
    PhantomData<(F, E)>,
);

impl<F: PrimeField, E: ExtensionField<F>, const NUM_BITS: usize, const LIMB_SIZE: usize>
    LassoSubtable<F, E> for ReminderSubtable<F, E, NUM_BITS, LIMB_SIZE>
{
    fn materialize(&self, M: usize) -> Vec<F> {
        assert_eq!(M, 1 << LIMB_SIZE);
        let remainder = NUM_BITS % LIMB_SIZE;
        let mut evals = vec![];
        (0..1 << remainder).for_each(|i| {
            evals.push(F::from(i));
        });
        evals
    }

    fn evaluate_mle(&self, point: &[E]) -> E {
        let b = point.len();
        let remainder = NUM_BITS % LIMB_SIZE;
        let mut result = E::ZERO;
        for i in 0..b {
            if i < remainder {
                result += point[b] * F::from(1u64 << (i));
            } else {
                result *= E::ONE - point[b];
            }
        }
        result
    }
}

#[derive(Clone, Debug, Default, Copy)]
pub struct RangeStategy<const NUM_BITS: usize, const LIMB_BITS: usize>;

impl<const NUM_BITS: usize, const LIMB_BITS: usize> LookupType
    for RangeStategy<NUM_BITS, LIMB_BITS>
{
    fn combine_lookups<F: PrimeField>(&self, operands: &[F]) -> F {
        let weight = F::from(1 << LIMB_BITS);
        inner_product(
            operands,
            iter::successors(Some(F::ONE), |power_of_weight| {
                Some(*power_of_weight * weight)
            })
            .take(operands.len())
            .collect_vec()
            .iter(),
        )
    }

    fn subtables<F: PrimeField, E: ExtensionField<F>>(
        &self,
    ) -> Vec<(Box<dyn LassoSubtable<F, E>>, SubtableIndices)> {
        let full = Box::new(FullLimbSubtable::<F, E, LIMB_BITS>(PhantomData));
        if NUM_BITS % LIMB_BITS == 0 {
            vec![(full, SubtableIndices::from(0))]
        } else {
            let rem = Box::new(ReminderSubtable::<F, E, NUM_BITS, LIMB_BITS>(PhantomData));
            vec![
                (full, SubtableIndices::from(0)),
                (rem, SubtableIndices::from(0)),
            ]
        }
    }

    fn output<F: PrimeField>(&self, index: &F) -> F {
        *index
    }

    fn chunk_bits(&self) -> Vec<usize> {
        let remainder_bits = if NUM_BITS % LIMB_BITS != 0 {
            vec![NUM_BITS % LIMB_BITS]
        } else {
            vec![]
        };
        iter::repeat(LIMB_BITS)
            .take(NUM_BITS / LIMB_BITS)
            .chain(remainder_bits)
            .collect_vec()
    }
    
    // fn to_indices<F: PrimeField>(&self, value: &F) -> Vec<usize> {
    //     chunk_operand_usize(value.into(), div_ceil(NUM_BITS, LIMB_BITS), LIMB_BITS)
    // }


    fn subtable_indices(&self, index_bits: Vec<bool>) -> Vec<Vec<bool>> {
        index_bits.chunks(LIMB_BITS).map(Vec::from).collect_vec()
    }
}

#[derive(Clone, Debug)]
pub struct RangeTable<F, E, const NUM_BITS: usize, const LIMB_BITS: usize>(
    PhantomData<F>,
    PhantomData<E>,
);

impl<F, E, const NUM_BITS: usize, const LIMB_BITS: usize> RangeTable<F, E, NUM_BITS, LIMB_BITS> {
    pub fn new() -> Self {
        Self(PhantomData, PhantomData)
    }
}

impl<F: PrimeField, E: ExtensionField<F>, const NUM_BITS: usize, const LIMB_BITS: usize>
    DecomposableTable<F, E> for RangeTable<F, E, NUM_BITS, LIMB_BITS>
{
    fn chunk_bits(&self) -> Vec<usize> {
        let remainder_bits = if NUM_BITS % LIMB_BITS != 0 {
            vec![NUM_BITS % LIMB_BITS]
        } else {
            vec![]
        };
        iter::repeat(LIMB_BITS)
            .take(NUM_BITS / LIMB_BITS)
            .chain(remainder_bits)
            .collect_vec()
    }

    fn combine_lookup_expressions(
        &self,
        expressions: Vec<Expression<E, usize>>,
    ) -> Expression<E, usize> {
        Expression::distribute_powers(expressions, E::from_bases(&[F::from(1 << LIMB_BITS)]))
    }

    fn subtables(&self) -> Vec<Box<dyn LassoSubtable<F, E>>> {
        let full = Box::new(FullLimbSubtable::<F, E, LIMB_BITS>(PhantomData));
        if NUM_BITS % LIMB_BITS == 0 {
            vec![full]
        } else {
            let rem = Box::new(ReminderSubtable::<F, E, NUM_BITS, LIMB_BITS>(PhantomData));
            vec![full, rem]
        }
    }

    fn combine_lookups(&self, operands: &[F]) -> F {
        let weight = F::from(1 << LIMB_BITS);
        inner_product(
            operands,
            iter::successors(Some(F::ONE), |power_of_weight| {
                Some(*power_of_weight * weight)
            })
            .take(operands.len())
            .collect_vec()
            .iter(),
        )
    }

    fn num_memories(&self) -> usize {
        div_ceil(NUM_BITS, LIMB_BITS)
    }

    fn subtable_indices(&self, index_bits: Vec<bool>) -> Vec<Vec<bool>> {
        index_bits.chunks(LIMB_BITS).map(Vec::from).collect_vec()
    }

    fn subtable_polys(&self) -> Vec<BoxMultilinearPoly<'static, F, E>> {
        let mut evals = vec![];
        (0..1 << LIMB_BITS).for_each(|i| evals.push(F::from(i)));
        let limb_subtable_poly = box_dense_poly(evals);
        if NUM_BITS % LIMB_BITS != 0 {
            let remainder = NUM_BITS % LIMB_BITS;
            let mut evals = vec![];
            (0..1 << remainder).for_each(|i| {
                evals.push(F::from(i));
            });
            let rem_subtable_poly = box_dense_poly(evals);
            vec![limb_subtable_poly, rem_subtable_poly]
        } else {
            vec![limb_subtable_poly]
        }
    }

    fn subtable_polys_terms(&self) -> Vec<MultilinearPolyTerms<F>> {
        let limb_init = PolyExpr::Var(0);
        let mut limb_terms = vec![limb_init];
        (1..LIMB_BITS).for_each(|i| {
            let coeff = PolyExpr::Pow(Box::new(PolyExpr::Const(F::from(2))), i as u32);
            let x = PolyExpr::Var(i);
            let term = PolyExpr::Prod(vec![coeff, x]);
            limb_terms.push(term);
        });
        let limb_subtable_poly = MultilinearPolyTerms::new(LIMB_BITS, PolyExpr::Sum(limb_terms));
        if NUM_BITS % LIMB_BITS == 0 {
            vec![limb_subtable_poly]
        } else {
            let remainder = NUM_BITS % LIMB_BITS;
            let rem_init = PolyExpr::Var(0);
            let mut rem_terms = vec![rem_init];
            (1..remainder).for_each(|i| {
                let coeff = PolyExpr::Pow(Box::new(PolyExpr::Const(F::from(2))), i as u32);
                let x = PolyExpr::Var(i);
                let term = PolyExpr::Prod(vec![coeff, x]);
                rem_terms.push(term);
            });
            vec![
                limb_subtable_poly,
                MultilinearPolyTerms::new(remainder, PolyExpr::Sum(rem_terms)),
            ]
        }
    }

    fn memory_to_chunk_index(&self, memory_index: usize) -> usize {
        memory_index
    }

    fn memory_to_subtable_index(&self, memory_index: usize) -> usize {
        if NUM_BITS % LIMB_BITS != 0 && memory_index == NUM_BITS / LIMB_BITS {
            1
        } else {
            0
        }
    }
}

pub fn chunk_operand_usize(x: u64, C: usize, chunk_len: usize) -> Vec<usize> {
    let bit_mask = (1 << chunk_len) - 1;
    (0..C)
        .map(|i| {
            let shift = ((C - i - 1) * chunk_len) as u32;
            (x.checked_shr(shift).unwrap_or(0) & bit_mask) as usize
        })
        .collect()
}

#[test]
fn range_test() {
    use goldilocks::Goldilocks;
    use itertools::izip;
    use plonkish_backend::util::arithmetic::{fe_to_bits_le, split_by_chunk_bits};

    let mut index_bits = fe_to_bits_le(Goldilocks::from_u128(100));

    // let chunk_bits = vec![16; 8]
    //     .iter()
    //     .map(|chunk_bits| chunk_bits / 2)
    //     .collect_vec();
    // let (lhs, rhs) = index_bits.split_at(index_bits.len() / 2);
    // let indices = izip!(
    //     split_by_chunk_bits(lhs, &chunk_bits),
    //     split_by_chunk_bits(rhs, &chunk_bits)
    // )
    // .map(|(chunked_lhs_bits, chunked_rhs_bits)| {
    //     iter::empty()
    //         .chain(chunked_lhs_bits)
    //         .chain(chunked_rhs_bits)
    //         .collect_vec()
    // })
    // .collect_vec();

    let table = RangeTable::<Goldilocks, Goldilocks, 64, 16>::new();

    println!("chunk_bits {:?}", table.chunk_bits());

    let indices = RangeTable::<Goldilocks, Goldilocks, 128, 16>::new().subtable_indices(index_bits);

    println!("{:?}", indices);
}

#[cfg(test)]
mod test {
    use halo2_curves::bn256;

    use super::*;

    #[test]
    fn and_test() {
        use goldilocks::Goldilocks;
        use itertools::izip;
        use plonkish_backend::util::arithmetic::{fe_to_bits_le, split_by_chunk_bits};

        let index_bits = fe_to_bits_le(bn256::Fr::from_u128(10));
        println!("{:?}", index_bits);

        let indices = and_subtable_indices(index_bits);

        println!("{:?}", indices);
    }

    fn and_subtable_indices(index_bits: Vec<bool>) -> Vec<Vec<bool>> {
        assert!(index_bits.len() % 2 == 0);
        let chunk_bits = vec![16; 8]
            .iter()
            .map(|chunk_bits| chunk_bits / 2)
            .collect_vec();
        let (lhs, rhs) = index_bits.split_at(index_bits.len() / 2);
        izip!(
            split_by_chunk_bits(lhs, &chunk_bits),
            split_by_chunk_bits(rhs, &chunk_bits)
        )
        .map(|(chunked_lhs_bits, chunked_rhs_bits)| {
            iter::empty()
                .chain(chunked_lhs_bits)
                .chain(chunked_rhs_bits)
                .collect_vec()
        })
        .collect_vec()
    }
}
