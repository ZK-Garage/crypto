// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Arithmetic Gates

use crate::proof_system::linearisation_poly::ProofEvaluations;
use ark_ff::FftField;
use ark_ff::{Field, PrimeField};
use ark_poly::polynomial::univariate::DensePolynomial;
use ark_poly::Evaluations;
use ark_poly_commit::{LabeledCommitment, PolynomialCommitment};
use ark_serialize::*;

/// Arithmetic Gates Prover Key
#[derive(CanonicalDeserialize, CanonicalSerialize, derivative::Derivative)]
#[derivative(Clone, Debug, Eq, PartialEq)]
pub struct ProverKey<F>
where
    F: FftField,
{
    /// Multiplication Selector
    pub q_m: (DensePolynomial<F>, Evaluations<F>),

    /// Left Wire Selector
    pub q_l: (DensePolynomial<F>, Evaluations<F>),

    /// Right Wire Selector
    pub q_r: (DensePolynomial<F>, Evaluations<F>),

    /// Output Wire Selector
    pub q_o: (DensePolynomial<F>, Evaluations<F>),

    /// Fourth Wire Selector
    pub q_4: (DensePolynomial<F>, Evaluations<F>),

    /// Constant Selector
    pub q_c: (DensePolynomial<F>, Evaluations<F>),

    /// Arithmetic Selector
    pub q_arith: (DensePolynomial<F>, Evaluations<F>),
}

impl<F> ProverKey<F>
where
    F: FftField,
{
    /// Computes the arithmetic gate contribution to the quotient polynomial at
    /// the element of the domain at the given `index`.
    pub fn compute_quotient_i(
        &self,
        index: usize,
        w_l_i: F,
        w_r_i: F,
        w_o_i: F,
        w_4_i: F,
    ) -> F {
        ((w_l_i * w_r_i * self.q_m.1[index])
            + (w_l_i * self.q_l.1[index])
            + (w_r_i * self.q_r.1[index])
            + (w_o_i * self.q_o.1[index])
            + (w_4_i * self.q_4.1[index])
            + self.q_c.1[index])
            * self.q_arith.1[index]
    }

    /// Computes the arithmetic gate contribution to the linearisation
    /// polynomial at the given evaluation points.
    pub fn compute_linearisation(
        &self,
        a_eval: F,
        b_eval: F,
        c_eval: F,
        d_eval: F,
        q_arith_eval: F,
    ) -> DensePolynomial<F> {
        &(&((&self.q_m.0 * (a_eval * b_eval))
            + (&self.q_l.0 * a_eval)
            + (&self.q_r.0 * b_eval)
            + (&self.q_o.0 * c_eval)
            + (&self.q_4.0 * d_eval))
            + &self.q_c.0)
            * q_arith_eval
    }
}

/// Arithmetic Gates Verifier Key
#[derive(CanonicalDeserialize, CanonicalSerialize, derivative::Derivative)]
#[derivative(Clone, /*Copy, Debug, Eq, PartialEq*/)]
pub struct VerifierKey<F, PC>
where
    F: PrimeField,
    PC: PolynomialCommitment<F, DensePolynomial<F>>,
{
    /// Multiplication Selector Commitment
    pub q_m: PC::Commitment,

    /// Left Selector Commitment
    pub q_l: PC::Commitment,

    /// Right Selector Commitment
    pub q_r: PC::Commitment,

    /// Output Selector Commitment
    pub q_o: PC::Commitment,

    /// Fourth Selector Commitment
    pub q_4: PC::Commitment,

    /// Constant Selector Commitment
    pub q_c: PC::Commitment,

    /// Arithmetic Selector Commitment
    pub q_arith: PC::Commitment,
}

impl<F, PC> VerifierKey<F, PC>
where
    F: PrimeField,
    PC: PolynomialCommitment<F, DensePolynomial<F>>,
{
    /// Computes arithmetic gate contribution to the linearisation polynomial
    /// commitment.
    pub fn compute_linearisation_commitment(
        &self,
        scalars: &mut Vec<F>,
        points: &mut Vec<PC::Commitment>,
        evaluations: &ProofEvaluations<F>,
    ) {
        let q_arith_eval = evaluations.q_arith_eval;

        scalars.push(evaluations.a_eval * evaluations.b_eval * q_arith_eval);
        points.push(self.q_m.clone());

        scalars.push(evaluations.a_eval * q_arith_eval);
        points.push(self.q_l.clone());

        scalars.push(evaluations.b_eval * q_arith_eval);
        points.push(self.q_r.clone());

        scalars.push(evaluations.c_eval * q_arith_eval);
        points.push(self.q_o.clone());

        scalars.push(evaluations.d_eval * q_arith_eval);
        points.push(self.q_4.clone());

        scalars.push(q_arith_eval);
        points.push(self.q_c.clone());
    }
}
