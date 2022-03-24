// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) ZK-Garage. All rights reserved.
//! Lookup gates

use crate::lookup::multiset::MultiSet;

use crate::proof_system::linearisation_poly::ProofEvaluations;
use crate::util::lc;
use ark_ff::{FftField, PrimeField};
use ark_poly::polynomial::univariate::DensePolynomial;
use ark_poly::Evaluations;
use ark_poly_commit::PCCommitment;
use ark_serialize::*;

/// Lookup Gates Prover Key
#[derive(CanonicalDeserialize, CanonicalSerialize, derivative::Derivative)]
#[derivative(Clone, Debug, Eq, PartialEq)]
pub struct ProverKey<F>
where
    F: PrimeField,
{
    /// Lookup selector
    pub q_lookup: (DensePolynomial<F>, Evaluations<F>),
    /// Column 1 of lookup table
    pub table_1: MultiSet<F>, // PC::Commitment, DensePolynomial<F>),
    /// Column 2 of lookup table
    pub table_2: MultiSet<F>, // PC::Commitment, DensePolynomial<F>),
    /// Column 3 of lookup table
    pub table_3: MultiSet<F>, // PC::Commitment, DensePolynomial<F>),
    /// Column 4 of lookup table
    pub table_4: MultiSet<F>, // PC::Commitment, DensePolynomial<F>),
}

impl<F> ProverKey<F>
where
    F: PrimeField,
{
    /// Compute lookup portion of quotient polynomial
    pub fn compute_quotient_i(
        &self,
        index: usize,
        w_l_i: F,
        w_r_i: F,
        w_o_i: F,
        w_4_i: F,
        f_i: F,
        table_i: F,
        table_i_next: F,
        h1_i: F,
        h1_i_next: F,
        h2_i: F,
        z2_i: F,
        z2_i_next: F,
        l1_i: F,
        delta: F,
        epsilon: F,
        zeta: F,
        lookup_sep: F,
    ) -> F {
        // q_lookup(X) * (a(X) + zeta * b(X) + (zeta^2 * c(X)) + (zeta^3 * d(X)
        // - f(X))) * α_1
        let q_lookup_i = self.q_lookup.1[index];
        let compressed_tuple = Self::compress(w_l_i, w_r_i, w_o_i, w_4_i, zeta);

        let lookup_sep_sq = lookup_sep.square();
        let lookup_sep_cu = lookup_sep_sq * lookup_sep;
        let one_plus_delta = delta + F::one();
        let epsilon_one_plus_delta = epsilon * one_plus_delta;

        let a = q_lookup_i * (compressed_tuple - f_i) * lookup_sep;

        // z2(X) * (1+δ) * (ε+f(X)) * (ε*(1+δ) + t(X) + δt(Xω)) * lookup_sep^2
        let b = {
            let b_1 = epsilon + f_i;
            let b_2 = epsilon_one_plus_delta + table_i + delta * table_i_next;

            z2_i * one_plus_delta * b_1 * b_2 * lookup_sep_sq
        };

        // − z2(Xω) * (ε*(1+δ) + h1(X) + δ*h2(X)) * (ε*(1+δ) + h2(X) + δ*h1(Xω))
        // * lookup_sep^2
        let c = {
            let c_1 = epsilon_one_plus_delta + h1_i + delta * h2_i;
            let c_2 = epsilon_one_plus_delta + h2_i + delta * h1_i_next;

            -z2_i_next * c_1 * c_2 * lookup_sep_sq
        };

        let d = { (z2_i - F::one()) * l1_i * lookup_sep_cu };

        a + b + c + d
    }

    /// Compute linearization for lookup gates
    pub(crate) fn compute_linearisation(
        &self,
        l1_eval: F,
        a_eval: F,
        b_eval: F,
        c_eval: F,
        d_eval: F,
        f_eval: F,
        table_eval: F,
        table_next_eval: F,
        h1_eval: F,
        h2_eval: F,
        z2_next_eval: F,
        delta: F,
        epsilon: F,
        zeta: F,
        z2_poly: &DensePolynomial<F>,
        h1_poly: &DensePolynomial<F>,
        lookup_separation_challenge: F,
    ) -> DensePolynomial<F> {
        let a = {
            let a_0 = Self::compress(a_eval, b_eval, c_eval, d_eval, zeta);
            &self.q_lookup.0 * ((a_0 - f_eval) * lookup_separation_challenge)
        };

        let lookup_sep_sq = lookup_separation_challenge.square();
        let lookup_sep_cu = lookup_separation_challenge * lookup_sep_sq;
        let one_plus_delta = delta + F::one();
        let epsilon_one_plus_delta = epsilon * one_plus_delta;

        // z2(X) * (1 + δ) * (ε + f_bar) * (ε(1+δ) + t_bar + δ*tω_bar) *
        // lookup_sep^2
        let b = {
            let b_0 = epsilon + f_eval;
            let b_1 =
                epsilon_one_plus_delta + table_eval + delta * table_next_eval;

            z2_poly * (one_plus_delta * b_0 * b_1 * lookup_sep_sq)
        };

        // h1(X) * (−z2ω_bar) * (ε(1+δ) + h2_bar  + δh1_bar) * lookup_sep^2
        let c = {
            let c_0 = -z2_next_eval * lookup_sep_sq;
            let c_1 = epsilon_one_plus_delta + h2_eval + delta * h1_eval;

            h1_poly * (c_0 * c_1)
        };

        let d = { z2_poly * (l1_eval * lookup_sep_cu) };

        a + b + c + d
    }

    /// Compresseses a row of values into a single field element
    /// by applying a random linear combination
    fn compress(w_l: F, w_r: F, w_o: F, w_4: F, zeta: F) -> F {
        lc(vec![w_l, w_r, w_o, w_4], zeta)
    }
}

/// Permutation Verifier Key
#[derive(CanonicalDeserialize, CanonicalSerialize, derivative::Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = "PCC: std::fmt::Debug"),
    Eq(bound = "PCC: Eq"),
    PartialEq(bound = "PCC: PartialEq")
)]
pub struct VerifierKey<PCC>
where
    PCC: PCCommitment + Default,
{
    /// Lookup Selector Commitment
    pub q_lookup: PCC,
}

impl<PCC> VerifierKey<PCC>
where
    PCC: PCCommitment + Default,
{
    /// Computes the linearisation commitments.
    pub fn compute_linearisation_commitment<F: FftField>(
        &self,
        scalars: &mut Vec<F>,
        points: &mut Vec<PCC>,
        evaluations: &ProofEvaluations<F>,
        (delta, epsilon, zeta): (F, F, F),
        lookup_sep: F,
        l1_eval: F,
        z2_comm: PCC,
        h1_comm: PCC,
    ) {
        let f = {
            let f_0 = evaluations.wire_evals.a_eval
                - evaluations.lookup_evals.f_eval
                + zeta * evaluations.wire_evals.b_eval
                + zeta * zeta * evaluations.wire_evals.c_eval
                + zeta * zeta * zeta * evaluations.wire_evals.d_eval;
            f_0 * lookup_sep
        };

        scalars.push(f);
        points.push(self.q_lookup.clone());

        // (1 + δ) * (ε + f_bar) * (ε(1+δ) + t_bar + δ*tω_bar) *  lookup_sep^2
        let one_plus_delta = F::one() + delta;
        let epsilon_one_plus_delta = epsilon * one_plus_delta;
        let lookup_sep_sq = lookup_sep.square();
        let lookup_sep_cu = lookup_sep_sq * lookup_sep;
        let z = {
            let z_0 = epsilon + evaluations.lookup_evals.f_eval;
            let z_1 = epsilon_one_plus_delta
                + evaluations.lookup_evals.table_eval
                + delta * evaluations.lookup_evals.table_next_eval;
            let z_2 = l1_eval * lookup_sep_cu;
            one_plus_delta * z_0 * z_1 * lookup_sep_sq + z_2
        };

        scalars.push(z);
        points.push(z2_comm);

        let lookup_sep_cu = lookup_sep_sq * lookup_sep;

        let w = {
            let w_0 = -evaluations.lookup_evals.z2_next_eval * lookup_sep_cu;
            let w_1 = epsilon_one_plus_delta
                + evaluations.lookup_evals.h2_eval
                + delta * evaluations.lookup_evals.h1_eval;
            w_0 * w_1
        };
        scalars.push(w);
        points.push(h1_comm);
    }
}
