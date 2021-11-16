// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Tools & traits for PLONK circuits

use core::marker::PhantomData;

use crate::constraint_system::StandardComposer;
use crate::error::Error;
use crate::proof_system::{
    Proof, Prover, ProverKey, Verifier, VerifierKey as PlonkVerifierKey,
};
use ark_ec::models::TEModelParameters;
use ark_ec::{
    twisted_edwards_extended::{GroupAffine, GroupProjective},
    PairingEngine, ProjectiveCurve,
};
use ark_ff::PrimeField;
use ark_poly::univariate::DensePolynomial;
use ark_poly_commit::kzg10::{self, Powers, UniversalParams};
use ark_poly_commit::sonic_pc::SonicKZG10;
use ark_poly_commit::PolynomialCommitment;
use ark_serialize::*;

// The reason for introducing these two traits is to have a workaround for not being able to
// implement `From<_> for Values` for both `PrimeField` and `GroupAffine`. The reason why this is
// not possible is because both the trait `PrimeField` and the struct `GroupAffine` are external
// to the crate, and therefore the compiler cannot be sure that `PrimeField` will never be
// implemented for `GroupAffine`. In which case, the two implementations of `From` would be
// inconsistent. To this end, we create to helper traits, `FeIntoValues` and `GeIntoValues`,
// that stand for "Field Element Into Values" and "Group Element Into Values" respectively.
trait FeIntoPubInput<F> {
    fn fe_into(self) -> F;
}

trait GeIntoPubInput<F> {
    fn ge_into(self) -> F;
}

#[derive(Default, Debug, Clone, CanonicalDeserialize, CanonicalSerialize)]
/// Structure that represents a PLONK Circuit Public Input converted into its
/// scalar representation.
pub struct PublicInputValue<F: PrimeField, P: TEModelParameters<BaseField = F>>
{
    pub(crate) values: Vec<F>,
    _marker: PhantomData<P>,
}

impl<F: PrimeField, P: TEModelParameters<BaseField = F>> FeIntoPubInput<PublicInputValue<F, P>> for F {
    fn fe_into(self) -> PublicInputValue<F, P> {
        PublicInputValue {
            values: vec![self],
            _marker: PhantomData,
        }
    }
}

impl<F: PrimeField, P: TEModelParameters<BaseField = F>> GeIntoPubInput<PublicInputValue<F, P>> for GroupAffine<P> {
    fn ge_into(self) -> PublicInputValue<F, P> {
        PublicInputValue{
            values: vec![self.x, self.y],
            _marker: PhantomData
        }
    }
}

impl<F: PrimeField, P: TEModelParameters<BaseField = F>> GeIntoPubInput<PublicInputValue<F, P>> for GroupProjective<P> {
    fn ge_into(self) -> PublicInputValue<F, P> {
        let point: GroupAffine<P> = self.into_affine();
        PublicInputValue{
            values: vec![point.x, point.y],
            _marker: PhantomData
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, CanonicalDeserialize, CanonicalSerialize)]
/// Collection of structs/objects that the Verifier will use in order to
/// de/serialize data needed for Circuit proof verification.
/// This structure can be seen as a link between the [`Circuit`] public input
/// positions and the [`VerifierKey`] that the Verifier needs to use.
pub struct VerifierData<
    E: PairingEngine,
    P: TEModelParameters<BaseField = E::Fr>,
> {
    key: PlonkVerifierKey<E, P>,
    pi_pos: Vec<usize>,
}

impl<E: PairingEngine, P: TEModelParameters<BaseField = E::Fr>>
    VerifierData<E, P>
{
    /// Creates a new `VerifierData` from a [`VerifierKey`] and the public
    /// input positions of the circuit that it represents.
    pub fn new(key: PlonkVerifierKey<E, P>, pi_pos: Vec<usize>) -> Self {
        Self { key, pi_pos }
    }

    /// Returns a reference to the contained [`VerifierKey`].
    pub fn key(self) -> PlonkVerifierKey<E, P> {
        self.key
    }

    /// Returns a reference to the contained Public Input positions.
    pub fn pi_pos(&self) -> &Vec<usize> {
        &self.pi_pos
    }
}

/// Trait that should be implemented for any circuit function to provide to it
/// the capabilities of automatically being able to generate, and verify proofs
/// as well as compile the circuit.
/// # Example
///
/// ```ignore
/// use rand_core::OsRng;
///
/// fn main() -> Result<(), Error> {
/// // Implements a circuit that checks:
/// // 1) a + b = c where C is a PI
/// // 2) a <= 2^6
/// // 3) b <= 2^5
/// // 4) a * b = d where D is a PI
/// // 5) JubJub::GENERATOR * e(JubJubScalar) = f where F is a PI
/// #[derive(Debug, Default)]
/// pub struct TestCircuit {
///     a: E::Fr,
///     b: E::Fr,
///     c: E::Fr,
///     d: E::Fr,
///     e: JubJubScalar,
///     f: JubJubAffine,
/// }
///
/// impl<
///         E: PairingEngine,
///         T: ProjectiveCurve<BaseField = E::Fr>,
///         P: TEModelParameters<BaseField = E::Fr>,
///     > Circuit<E, T, P> for TestCircuit<E, T, P>
/// {
///     const CIRCUIT_ID: [u8; 32] = [0xff; 32];
///     fn gadget(
///         &mut self,
///         composer: &mut StandardComposer<E, T, P>,
///     ) -> Result<(), Error> {
///         // Add fixed witness zero
///         let a = composer.add_input(self.a);
///         let b = composer.add_input(self.b);
///         // Make first constraint a + b = c
///         composer.poly_gate(
///             a,
///             b,
///             composer.zero_var,
///             E::Fr::zero(),
///             E::Fr::one(),
///             E::Fr::one(),
///             E::Fr::zero(),
///             E::Fr::zero(),
///             Some(-self.c),
///         );
///         // Check that a and b are in range
///         composer.range_gate(a, 1 << 6);
///         composer.range_gate(b, 1 << 5);
///         // Make second constraint a * b = d
///         composer.poly_gate(
///             a,
///             b,
///             composer.zero,
///             E::Fr::one(),
///             E::Fr::zero(),
///             E::Fr::zero(),
///             E::Fr::one(),
///             E::Fr::zero(),
///             Some(-self.d),
///         );
///
///         let e = composer
///             .add_input(util::from_embedded_curve_scalar::<E, P>(self.e));
///         let (x, y) = P::AFFINE_GENERATOR_COEFFS;
///         let generator = GroupAffine::new(x, y);
///         let scalar_mul_result =
///             composer.fixed_base_scalar_mul(e, generator);
///         // Apply the constrain
///         composer
///             .assert_equal_public_point(scalar_mul_result, self.f);
///         Ok(())
///     }
///     fn padded_circuit_size(&self) -> usize {
///         1 << 11
///     }
/// }
///
/// let pp = PublicParameters::setup(1 << 12, &mut OsRng)?;
/// // Initialize the circuit
/// let mut circuit = TestCircuit::default();
/// // Compile the circuit
/// let (pk, vd) = circuit.compile(&pp)?;
///
/// // Prover POV
/// let (x, y) = P::AFFINE_GENERATOR_COEFFS;
/// let generator = GroupAffine::new(x, y);
/// let proof = {
///     let mut circuit = TestCircuit {
///         a: E::Fr::from(20u64),
///         b: E::Fr::from(5u64),
///         c: E::Fr::from(25u64),
///         d: E::Fr::from(100u64),
///         e: JubJubScalar::from(2u64),
///         f: JubJubAffine::from(
///             generator * JubJubScalar::from(2u64),
///         ),
///         _marker: PhantomData,
///     };
///
///     circuit.gen_proof(&pp, &pk, b"Test")
/// }?;
///
/// // Verifier POV
/// let public_inputs: Vec<PublicInputValue> = vec![
///     E::Fr::from(25u64).into(),
///     E::Fr::from(100u64).into(),
///     JubJubAffine::from(
///         generator * JubJubScalar::from(2u64),
///     )
///     .into(),
/// ];
///
/// circuit::verify_proof(
///     &pp,
///     &vd.key(),
///     &proof,
///     &public_inputs,
///     &vd.pi_pos(),
///     b"Test",
/// )
/// }
pub trait Circuit<E, T, P>
where
    E: PairingEngine,
    T: ProjectiveCurve<BaseField = E::Fr>,
    P: TEModelParameters<BaseField = E::Fr>,
    Self: Sized,
{
    /// Circuit identifier associated constant.
    const CIRCUIT_ID: [u8; 32];
    /// Gadget implementation used to fill the composer.
    fn gadget(
        &mut self,
        composer: &mut StandardComposer<E, T, P>,
    ) -> Result<(), Error>;
    /// Compiles the circuit by using a function that returns a `Result`
    /// with the `ProverKey`, `VerifierKey` and the circuit size.
    fn compile(
        &mut self,
        u_params: &UniversalParams<E>,
    ) -> Result<(ProverKey<E::Fr, P>, VerifierData<E, P>), Error> {
        // Setup PublicParams
        // XXX: KZG10 does not have a trim function so we use sonics and
        // then do a transformation between sonic CommiterKey to KZG10
        // powers
        let circuit_size = self.padded_circuit_size();
        let (ck, _) = SonicKZG10::<E, DensePolynomial<E::Fr>>::trim(
            u_params,
            circuit_size,
            0,
            None,
        )
        .unwrap();
        let powers = Powers {
            powers_of_g: ck.powers_of_g.into(),
            powers_of_gamma_g: ck.powers_of_gamma_g.into(),
        };
        //Generate & save `ProverKey` with some random values.
        let mut prover = Prover::new(b"CircuitCompilation");
        self.gadget(prover.mut_cs())?;
        let pi_pos = prover.mut_cs().pi_positions();
        prover.preprocess(&powers)?;

        // Generate & save `VerifierKey` with some random values.
        let mut verifier = Verifier::new(b"CircuitCompilation");
        self.gadget(verifier.mut_cs())?;
        verifier.preprocess(&powers)?;
        Ok((
            prover
                .prover_key
                .expect("Unexpected error. Missing ProverKey in compilation"),
            VerifierData::new(
                verifier.verifier_key.expect(
                    "Unexpected error. Missing VerifierKey in compilation",
                ),
                pi_pos,
            ),
        ))
    }

    /// Generates a proof using the provided `CircuitInputs` & `ProverKey`
    /// instances.
    fn gen_proof(
        &mut self,
        u_params: &UniversalParams<E>,
        prover_key: ProverKey<E::Fr, P>,
        transcript_init: &'static [u8],
    ) -> Result<Proof<E, P>, Error> {
        // XXX: KZG10 does not have a trim function so we use sonics and
        // then do a transformation between sonic CommiterKey to KZG10
        // powers
        let circuit_size = self.padded_circuit_size();
        let (ck, _) = SonicKZG10::<E, DensePolynomial<E::Fr>>::trim(
            u_params,
            circuit_size,
            0,
            None,
        )
        .unwrap();
        let powers = Powers {
            powers_of_g: ck.powers_of_g.into(),
            powers_of_gamma_g: ck.powers_of_gamma_g.into(),
        };
        // New Prover instance
        let mut prover = Prover::new(transcript_init);
        // Fill witnesses for Prover
        self.gadget(prover.mut_cs())?;
        // Add ProverKey to Prover
        prover.prover_key = Some(prover_key);
        prover.prove(&powers)
    }

    /// Returns the Circuit size padded to the next power of two.
    fn padded_circuit_size(&self) -> usize;
}

/// Verifies a proof using the provided `CircuitInputs` & `VerifierKey`
/// instances.
pub fn verify_proof<
    E: PairingEngine,
    T: ProjectiveCurve<BaseField = E::Fr>,
    P: TEModelParameters<BaseField = E::Fr>,
>(
    u_params: &UniversalParams<E>,
    plonk_verifier_key: PlonkVerifierKey<E, P>,
    proof: &Proof<E, P>,
    pub_inputs_values: &[PublicInputValue<E::Fr, P>],
    pub_inputs_positions: &[usize],
    transcript_init: &'static [u8],
) -> Result<(), Error> {
    let mut verifier: Verifier<E, T, P> = Verifier::new(transcript_init);
    let padded_circuit_size = plonk_verifier_key.padded_circuit_size();
    // let key: VerifierKey<E, P> = *plonk_verifier_key;
    verifier.verifier_key = Some(plonk_verifier_key);
    let (_, sonic_vk) = SonicKZG10::<E, DensePolynomial<E::Fr>>::trim(
        u_params,
        padded_circuit_size,
        0,
        None,
    )
    .unwrap();

    let vk = kzg10::VerifierKey {
        g: sonic_vk.g,
        gamma_g: sonic_vk.gamma_g,
        h: sonic_vk.h,
        beta_h: sonic_vk.beta_h,
        prepared_h: sonic_vk.prepared_h,
        prepared_beta_h: sonic_vk.prepared_beta_h,
    };

    verifier.verify(
        proof,
        &vk,
        build_pi(pub_inputs_values, pub_inputs_positions, padded_circuit_size)
            .as_slice(),
    )
}

/// Build PI vector for Proof verifications.
fn build_pi<F: PrimeField, P: TEModelParameters<BaseField = F>>(
    pub_input_values: &[PublicInputValue<F, P>],
    pub_input_pos: &[usize],
    trim_size: usize,
) -> Vec<F> {
    let mut pi = vec![F::zero(); trim_size];
    pub_input_values
        .iter()
        .map(|pub_input| pub_input.values.clone())
        .flatten()
        .zip(pub_input_pos.iter().copied())
        .for_each(|(value, pos)| {
            pi[pos] = -value;
        });
    pi
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{constraint_system::StandardComposer, util};
    use ark_bls12_381::{Fr as BlsScalar, Bls12_381};
    use ark_ec::twisted_edwards_extended::GroupAffine;
    use ark_ed_on_bls12_381::{Fr as EmbeddedScalar, EdwardsParameters, EdwardsProjective, EdwardsAffine};
    use num_traits::{One, Zero};
    use ark_poly_commit::kzg10::KZG10;
    use ark_ec::AffineCurve;

    // Implements a circuit that checks:
    // 1) a + b = c where C is a PI
    // 2) a <= 2^6
    // 3) b <= 2^5
    // 4) a * b = d where D is a PI
    // 5) JubJub::GENERATOR * e(JubJubScalar) = f where F is a PI
    #[derive(Debug, Default)]
    pub struct TestCircuit<
        E: PairingEngine,
        T: ProjectiveCurve<BaseField = E::Fr>,
        P: TEModelParameters<BaseField = E::Fr>,
    > {
        a: E::Fr,
        b: E::Fr,
        c: E::Fr,
        d: E::Fr,
        e: P::ScalarField,
        f: GroupAffine<P>,
        _marker: PhantomData<T>,
    }

    impl<
            E: PairingEngine,
            T: ProjectiveCurve<BaseField = E::Fr>,
            P: TEModelParameters<BaseField = E::Fr>,
        > Circuit<E, T, P> for TestCircuit<E, T, P>
    {
        const CIRCUIT_ID: [u8; 32] = [0xff; 32];
        fn gadget(
            &mut self,
            composer: &mut StandardComposer<E, T, P>,
        ) -> Result<(), Error> {
            let a = composer.add_input(self.a);
            let b = composer.add_input(self.b);
            // Make first constraint a + b = c
            composer.add(
                (E::Fr::zero(), a),
                (E::Fr::zero(), b),
                E::Fr::zero(),
                Some(-self.c),
            );
            // Check that a and b are in range
            composer.range_gate(a, 1 << 6);
            composer.range_gate(b, 1 << 5);
            // Make second constraint a * b = d
            composer.mul(E::Fr::one(), a, b, E::Fr::zero(), Some(-self.d));

            let e = composer
                .add_input(util::from_embedded_curve_scalar::<E, P>(self.e));
            let (x, y) = P::AFFINE_GENERATOR_COEFFS;
            let generator = GroupAffine::new(x, y);
            let scalar_mul_result =
                composer.fixed_base_scalar_mul(e, generator);
            // Apply the constrain
            composer.assert_equal_public_point(scalar_mul_result, self.f);
            Ok(())
        }
        fn padded_circuit_size(&self) -> usize {
            1 << 11
        }
    }

    // TODO: Complete serialization first
    #[test]
    fn test_full() {
        use rand_core::OsRng;

        // Generate CRS
        let pp = KZG10::<Bls12_381,DensePolynomial<BlsScalar>,>::setup(
            1 << 12, false, &mut OsRng
        ).unwrap();

        let (x, y) = EdwardsParameters::AFFINE_GENERATOR_COEFFS;
        let generator = EdwardsAffine::new(x, y);
        let point_f_pi: EdwardsAffine = AffineCurve::mul(
            &generator,
            EmbeddedScalar::from(2u64).into_repr(),
        ).into_affine();

        // Prover POV
        let (proof, verifier_data) = {

            let mut circuit: TestCircuit<
                Bls12_381,
                EdwardsProjective,
                EdwardsParameters,
            > = TestCircuit {
                a: BlsScalar::from(20u64),
                b: BlsScalar::from(5u64),
                c: BlsScalar::from(25u64),
                d: BlsScalar::from(100u64),
                e: EmbeddedScalar::from(2u64),
                f: point_f_pi,
                _marker: PhantomData,
            };

            // Compile the circuit
            // todo: maybe this can be done out of the prover scope? How should we initialise the circuit then?
            let (pk_p, og_verifier_data) = circuit.compile(&pp).unwrap();

            (circuit.gen_proof(&pp, pk_p, b"Test").unwrap(), og_verifier_data)
        };

        // Test serialisation for verifier_data
        let mut verifier_data_bytes = Vec::new();
        verifier_data.serialize(&mut verifier_data_bytes).unwrap();

        let verif_data: VerifierData<Bls12_381, EdwardsParameters> = VerifierData::deserialize(verifier_data_bytes.as_slice()).unwrap();

        assert!(verif_data == verifier_data);

        // Verifier POV
        let public_inputs: Vec<PublicInputValue<BlsScalar, EdwardsParameters>> = vec![
            BlsScalar::from(25u64).fe_into(),
            BlsScalar::from(100u64).fe_into(),
            point_f_pi.ge_into(),
        ];

        // todo: non-ideal hack for a first functional version.
        let pi_pos = verifier_data.pi_pos().clone();
        assert!(verify_proof::<Bls12_381, EdwardsProjective, EdwardsParameters>(
            &pp,
            verifier_data.key(),
            &proof,
            &public_inputs,
            &pi_pos,
            b"Test",
        ).is_ok());
    }
}
