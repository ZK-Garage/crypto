// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::error::Error;
use ark_ff::{Field, PrimeField};
use ark_poly::{
    univariate::DensePolynomial, EvaluationDomain, GeneralEvaluationDomain,
    UVPolynomial,
};
use ark_serialize::{
    CanonicalDeserialize, CanonicalSerialize, Read, SerializationError, Write,
};
use core::ops::{Add, Mul};
use hashbrown::HashMap;

/// MultiSet is struct containing vectors of scalars, which
/// individually represents either a wire value or an index
/// of a PlookUp table
#[derive(
    CanonicalDeserialize,
    CanonicalSerialize,
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
)]
pub struct MultiSet<F>(pub Vec<F>)
where
    F: Field;

impl<F> MultiSet<F>
where
    F: Field,
{
    /// Creates an empty vector with a multiset wrapper around it
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a `MultiSet` witch capacity for `len` elements
    pub fn with_capacity(len: usize) -> Self {
        MultiSet(Vec::with_capacity(len))
    }

    /// Creates a `MultiSet` of lenght `len` with zero elements
    pub fn with_len(len: usize) -> Self {
        MultiSet(vec![F::zero(); len])
    }

    /// Generate a `MultiSet` struct from a slice of bytes.
    pub fn from_slice(_bytes: &[u8]) -> Result<Self, Error> {
        /* FIXME: Find correct implementation.
            let mut buffer = bytes;
            let elements = buffer
                .chunks(F::SIZE)
                .map(F::from_slice)
                .collect::<Result<_, Error>>()?;
            Ok(Self(elements))
        */
        todo!()
    }

    /// Given a [`MultiSet`], return it in it's bytes representation element by
    /// element.
    pub fn to_var_bytes(&self) -> Vec<u8> {
        self.0
            .iter()
            .flat_map(|item| {
                // FIXME: Is there a better way to do this in arkworks?
                let mut bytes = vec![];
                item.write(&mut bytes).expect("This never fails.");
                bytes
            })
            .collect()
    }

    /// Extends the length of the multiset to n elements The n will be the size
    /// of the arithmetic circuit. This will extend the vectors to the size
    pub fn pad(&mut self, n: u32) {
        assert!(n.is_power_of_two());
        if self.is_empty() {
            self.push(F::zero())
        };
        let diff = n - self.len() as u32;
        self.0.extend(vec![self.0[0]; diff as usize]);
    }

    /// Pushes chosen value onto the end of the Multiset
    pub fn push(&mut self, value: F) {
        self.0.push(value)
    }

    /// Extendes values onto the end of the Multiset
    pub fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = F>,
    {
        self.0.extend(iter)
    }
    /// Fetches last element in MultiSet.
    /// Returns None if there are no elements in the MultiSet.
    pub fn last(&self) -> Option<&F> {
        self.0.last()
    }

    /// Returns the cardinality of the multiset
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether or not the multiset is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the position of the element in the Multiset.
    /// Returns None if the element is not found.
    pub fn position(&self, element: &F) -> Option<usize> {
        self.0.iter().position(move |x| x == element)
    }

    /// Concatenates and sorts two Multisets together.
    ///
    /// All elements of f must exist in t. TODO Define Error for this case.
    /// From the Plookup paper, if we have t: {1,2,4,3} and f: {2,3,4,1}.
    /// We combine the multisets together and sort their elements together.
    /// The final MultiSet will look as follows, s: {1,1,2,2,3,3,4,4}.
    /// Splits a multiset into alternating halves of the same length (if even).
    pub fn sorted_halve(&self, f: &Self) -> Result<(Self, Self), Error> {
        let mut counters: HashMap<F, usize> = HashMap::new();

        let n_elems = self.len() + f.len();
        // Insert elemnts on of t in sorted struct
        let mut val;
        for element in &self.0 {
            match counters.get(element) {
                Some(v) => val = v + 1,
                _ => val = 1,
            };
            counters.insert(*element, val);
        }

        // Insert elemnts on of f in sorted struct + check they are in t
        for element in &f.0 {
            match counters.get_mut(element) {
                Some(entry) => *entry += 1,
                // TODO Raise Error indicating some element of `f` not in `t`
                _ => todo!(),
            }
        }

        let mut evens = Vec::with_capacity(n_elems + (n_elems % 2));
        let mut odds = Vec::with_capacity(n_elems);
        let mut parity = 0;
        self.0.iter().for_each(|elem| {
            let count = counters.get_mut(elem).unwrap();
            let half_count = *count / 2;
            evens.extend(vec![*elem; half_count]);
            odds.extend(vec![*elem; half_count]);
            // if odd count => add extra element to corresponding
            // vec and flip prev_parity
            if *count % 2 == 1 {
                if parity == 1 {
                    odds.push(*elem);
                    parity = 0;
                } else {
                    evens.push(*elem);
                    parity = 1
                }
            }
            *count = 0usize;
        });

        Ok((Self(evens), Self(odds)))
    }

    /// Splits a multiset into alternating halves of the same length
    /// as specified in the Plonkup paper. A multiset must have even
    /// cardinality to be split in this manner.
    pub fn halve_alternating(&self) -> (Self, Self) {
        let mut evens = vec![];
        let mut odds = vec![];
        for i in 0..self.len() {
            if i % 2 == 0 {
                evens.push(self.0[i]);
            } else {
                odds.push(self.0[i]);
            }
        }
        (Self(evens), Self(odds))
    }

    /// Checks whether one mutltiset is a subset of another.
    /// This function will be used to check if the all elements
    /// in set f, from the paper, are contained inside t.
    pub fn contains_all(&self, other: &Self) -> bool {
        // TODO: Use a more optimal algorithm, should probably be able to do
        // `O(nlogn)`.
        // We can improve
        other.0.iter().all(|item| self.contains(item))
    }

    /// Checks if an element is in the MultiSet
    pub fn contains(&self, entry: &F) -> bool {
        self.0.contains(entry)
    }

    /// Splits a multiset into halves as specified by the paper
    /// The last element of the first half should be the same
    /// as the first element of the second half.
    /// Since a multiset can never have an even cardinality, we
    /// always split it in the way described above.
    pub fn halve(&self) -> (Self, Self) {
        let length = self.0.len();
        let first_half = Self::from(&self.0[..=length / 2]);
        let second_half = Self::from(&self.0[length / 2..]);
        (first_half, second_half)
    }

    /// Treats each element in the multiset as evaluation points
    /// Computes IFFT of the set of evaluation points
    /// and returns the coefficients as a Polynomial data structure
    pub(crate) fn to_polynomial(
        &self,
        domain: &GeneralEvaluationDomain<F>,
    ) -> DensePolynomial<F>
    where
        F: PrimeField,
    {
        DensePolynomial::from_coefficients_vec(domain.ifft(&self.0))
    }

    /// Compress a vector of multisets into a single multiset using
    /// a RLC. A random challenge `alpha` needs to be provided. It
    /// is dervived by hashing the transcript.
    pub fn compress(multisets: &[Self], alpha: F) -> Self {
        let len = multisets[0].0.len();
        for mset in multisets.iter().skip(1) {
            assert_eq!(mset.0.len(), len)
        }
        multisets
            .iter()
            .rev()
            .fold(MultiSet::with_len(len), |acc, m| acc * alpha + m.clone())
    }

    /// Concatenates with a new `MultiSet` and sort
    pub fn sorted_concat(&mut self, other: Self) {
        // TODO This could be a performance bottleneck
        self.extend(other.0);
        self.0.sort();
    }
}

impl<F> From<&[F]> for MultiSet<F>
where
    F: Field,
{
    #[inline]
    fn from(slice: &[F]) -> Self {
        Self(slice.to_vec())
    }
}

impl<F> FromIterator<F> for MultiSet<F>
where
    F: Field,
{
    #[inline]
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = F>,
    {
        Self(Vec::from_iter(iter))
    }
}

/// Elementwise addtion
impl<F> Add for MultiSet<F>
where
    F: Field,
{
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        self.0
            .into_iter()
            .zip(other.0.iter())
            .map(|(x, y)| x + y)
            .collect()
    }
}

/// Elementwise multiplication
impl<F> Mul<MultiSet<F>> for MultiSet<F>
where
    F: Field,
{
    type Output = Self;

    fn mul(self, other: Self) -> Self::Output {
        self.0
            .into_iter()
            .zip(other.0.iter())
            .map(|(x, y)| x * y)
            .collect()
    }
}

/// Multiplication with a field element
impl<F> Mul<F> for MultiSet<F>
where
    F: Field,
{
    type Output = Self;
    fn mul(self, elem: F) -> Self::Output {
        self.0.into_iter().map(|x| x * elem).collect()
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::batch_field_test;
    use crate::lookup::WitnessTable;
    use ark_bls12_377::Fr as bls12_377_scalar_field;
    use ark_bls12_381::Fr as bls12_381_scalar_field;
    use ark_poly::EvaluationDomain;
    use ark_poly::Polynomial;

    // FIXME: Run tests on both BLS fields.

    fn test_halve<F>()
    where
        F: Field,
    {
        let mut s = MultiSet::new();
        s.push(F::from(0u32));
        s.push(F::from(1u32));
        s.push(F::from(2u32));
        s.push(F::from(3u32));
        s.push(F::from(4u32));
        s.push(F::from(5u32));
        s.push(F::from(6u32));

        let (h_1, h_2) = s.halve();
        assert_eq!(h_1.len(), 4);
        assert_eq!(h_2.len(), 4);

        let left_half = MultiSet(vec![
            F::from(0u32),
            F::from(1u32),
            F::from(2u32),
            F::from(3u32),
        ]);

        assert_eq!(left_half, h_1);

        let right_half = MultiSet(vec![
            F::from(3u32),
            F::from(4u32),
            F::from(5u32),
            F::from(6u32),
        ]);

        assert_eq!(right_half, h_2);

        // The last element of the first half should equal the first
        // element of the second half.
        assert_eq!(h_1.0.last().unwrap(), &h_2.0[0])
    }

    fn test_to_polynomial<F>()
    where
        F: PrimeField,
    {
        let mut s = MultiSet::new();
        s.push(F::from(1u32));
        s.push(F::from(2u32));
        s.push(F::from(3u32));
        s.push(F::from(4u32));
        s.push(F::from(5u32));
        s.push(F::from(6u32));
        s.push(F::from(7u32));

        let domain = EvaluationDomain::new(s.len() + 1).unwrap();
        let s_poly = s.to_polynomial(&domain);

        assert_eq!(s_poly.degree(), 7)
    }

    fn test_is_subset<F>()
    where
        F: Field,
    {
        let mut t = MultiSet::new();
        t.push(F::from(1u32));
        t.push(F::from(2u32));
        t.push(F::from(3u32));
        t.push(F::from(4u32));
        t.push(F::from(5u32));
        t.push(F::from(6u32));
        t.push(F::from(7u32));

        let mut f = MultiSet::new();
        f.push(F::from(1u32));
        f.push(F::from(2u32));

        let mut n = MultiSet::new();
        n.push(F::from(8u32));

        assert!(t.contains_all(&f));
        assert!(!t.contains_all(&n));
    }

    fn test_full_compression_into_s<F>()
    where
        F: Field,
    {
        let mut t = MultiSet::new();
        t.push(F::zero());
        t.push(F::one());
        t.push(F::from(2u32));
        t.push(F::from(3u32));
        t.push(F::from(4u32));
        t.push(F::from(5u32));
        t.push(F::from(6u32));
        t.push(F::from(7u32));

        let mut f = MultiSet::new();
        f.push(F::from(3u32));
        f.push(F::from(6u32));
        f.push(F::from(0u32));
        f.push(F::from(5u32));
        f.push(F::from(4u32));
        f.push(F::from(3u32));
        f.push(F::from(2u32));
        f.push(F::from(0u32));
        f.push(F::from(0u32));
        f.push(F::from(1u32));
        f.push(F::from(2u32));

        assert!(t.contains_all(&f));
        assert!(t.contains(&F::from(2u32)));

        t.sorted_concat(f);

        // The sets should be merged but also
        // in the ascending order
        let concatenated_set = MultiSet(vec![
            F::zero(),
            F::zero(),
            F::zero(),
            F::zero(),
            F::one(),
            F::one(),
            F::from(2u32),
            F::from(2u32),
            F::from(2u32),
            F::from(3u32),
            F::from(3u32),
            F::from(3u32),
            F::from(4u32),
            F::from(4u32),
            F::from(5u32),
            F::from(5u32),
            F::from(6u32),
            F::from(6u32),
            F::from(7u32),
        ]);

        assert_eq!(t, concatenated_set);
    }

    // TODO Delete if not used
    fn _multiset_compression_input<F>()
    where
        F: Field,
    {
        // Alpha is a random challenge from
        // the transcript
        let alpha = F::from(2u32);
        let alpha_squared = alpha * alpha;

        let mut table = WitnessTable::default();

        // Fill in wires directly, no need to use a
        // plookup table as this will not be going
        // into a proof
        table.from_wire_values(vec![
            F::from(1u32),
            F::from(2u32),
            F::from(3u32),
            F::from(3u32),
        ]);

        // Computed expected result
        let compressed_element = MultiSet::compress(&table.f, alpha);

        let actual_element = F::from(1u32)
            + (F::from(2u32) * alpha)
            + (F::from(3u32) * alpha_squared);

        let mut actual_set = MultiSet::new();

        actual_set.push(actual_element);

        assert_eq!(actual_set, compressed_element);
    }

    // Bls12-381 tests
    batch_field_test!(
        [
            test_halve,
            test_to_polynomial,
            test_is_subset,
            test_full_compression_into_s
        ],
        [] => bls12_381_scalar_field
    );

    // Bls12-377 tests
    batch_field_test!(
        [
            test_halve,
            test_to_polynomial,
            test_is_subset,
            test_full_compression_into_s
        ],
        [] => bls12_377_scalar_field
    );
}
