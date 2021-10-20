// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use ark_ec::{AffineCurve, PairingEngine, TEModelParameters};
use ark_poly_commit::kzg10::Commitment;
use core::marker::PhantomData;

use crate::transcript::{TranscriptProtocol, TranscriptWrapper};

#[derive(Copy, Clone, Debug)]
/// Proof that a polynomial `p` was correctly evaluated at a point `z`
/// producing the evaluated point p(z).
pub struct KZGProof<E: PairingEngine> {
    /// This is a commitment to the witness polynomial.
    pub(crate) commitment_to_witness: Commitment<E>,
    /// This is the result of evaluating a polynomial at the point `z`.
    pub(crate) evaluated_point: E::Fr,
    /// This is the commitment to the polynomial that you want to prove a
    /// statement about.
    pub(crate) commitment_to_polynomial: Commitment<E>,
}

/// Proof that multiple polynomials were correctly evaluated at a point `z`,
/// each producing their respective evaluated points p_i(z).
#[derive(Debug)]
pub(crate) struct KZGAggregateProof<
    E: PairingEngine,
    P: TEModelParameters<BaseField = E::Fr>,
> {
    /// This is a commitment to the aggregated witness polynomial.
    /// The aggregate witness polynomial is a linear combination of the
    /// witness polynomials (p_i(X) - p_i(z)) / (X-z)
    pub(crate) commitment_to_witness: Commitment<E>,
    /// These are the results of the evaluating each polynomial p_i at the
    /// point `z`.
    pub(crate) evaluated_points: Vec<E::Fr>,
    /// These are the commitments to the p_i polynomials.
    pub(crate) commitments_to_polynomials: Vec<Commitment<E>>,
    pub(crate) _marker: PhantomData<P>,
}

impl<E: PairingEngine, P: TEModelParameters<BaseField = E::Fr>>
    KZGAggregateProof<E, P>
{
    /// Initialises an `AggregatedProof` with the commitment to the witness.
    pub(crate) fn with_witness(
        witness: Commitment<E>,
    ) -> KZGAggregateProof<E, P> {
        KZGAggregateProof {
            commitment_to_witness: witness,
            evaluated_points: Vec::new(),
            commitments_to_polynomials: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Adds an evaluated point with the commitment to the polynomial which
    /// produced it.
    pub(crate) fn add_part(&mut self, part: (E::Fr, Commitment<E>)) {
        self.evaluated_points.push(part.0);
        self.commitments_to_polynomials.push(part.1);
    }

    /// Flattens an `KZGAggregateProof` into a `Proof`.
    /// The transcript must have the same view as the transcript that was
    /// used to aggregate the witness in the proving stage.
    pub(crate) fn flatten(
        &self,
        transcript: &mut TranscriptWrapper<E>,
    ) -> KZGProof<E> {
        let challenge: E::Fr =
            transcript.challenge_scalar(b"aggregate_witness");
        let powers: Vec<E::Fr> = crate::util::powers_of(
            &challenge,
            self.commitments_to_polynomials.len() - 1,
        );

        let flattened_poly_commitments_iter =
            self.commitments_to_polynomials.iter().zip(powers.iter());
        let flattened_poly_evaluations_iter =
            self.evaluated_points.iter().zip(powers.iter());

        // Flattened polynomial commitments using challenge
        let flattened_poly_commitments = Commitment(
            flattened_poly_commitments_iter
                .map(|(poly_commitment, challenge_power)| {
                    (poly_commitment.0).mul(*challenge_power)
                })
                .sum::<E::G1Projective>()
                .into(),
        );
        //);
        // Flattened evaluation points
        let flattened_poly_evaluations: E::Fr = flattened_poly_evaluations_iter
            .map(|(eval, challenge_power)| *challenge_power * eval)
            .sum();

        KZGProof::<E> {
            commitment_to_witness: self.commitment_to_witness,
            evaluated_point: flattened_poly_evaluations,
            commitment_to_polynomial: Commitment::from(
                flattened_poly_commitments,
            ),
        }
    }
}
