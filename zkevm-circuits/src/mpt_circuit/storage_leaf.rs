use eth_types::Field;
use gadgets::util::Scalar;
use halo2_proofs::{
    circuit::{Region, Value},
    plonk::{Error, VirtualCells},
    poly::Rotation,
};

use crate::circuit_tools::cell_manager::Cell;
use crate::circuit_tools::constraint_builder::{RLCChainable, RLCableValue};
use crate::circuit_tools::gadgets::{LtGadget, RequireNotZeroGadget};
use crate::mpt_circuit::helpers::{drifted_nibble_rlc, IsEmptyTreeGadget};
use crate::table::ProofType;
use crate::{
    assign, circuit,
    mpt_circuit::{
        helpers::{key_memory, parent_memory, KeyData, MPTConstraintBuilder, ParentData},
        param::{HASH_WIDTH, IS_NON_EXISTING_STORAGE_POS, IS_STORAGE_MOD_POS, KEY_LEN_IN_NIBBLES},
        FixedTableTag,
    },
    mpt_circuit::{
        witness_row::{MptWitnessRow, MptWitnessRowType},
        MPTContext,
    },
    mpt_circuit::{MPTConfig, ProofValues},
};

use super::helpers::{Indexable, LeafKeyGadget, RLPValueGadget};

#[derive(Clone, Debug, Default)]
pub(crate) struct StorageLeafConfig<F> {
    key_data: [KeyData<F>; 2],
    key_data_w: KeyData<F>,
    parent_data: [ParentData<F>; 2],
    key_mult: [Cell<F>; 2],
    drifted_mult: Cell<F>,
    rlp_key: [LeafKeyGadget<F>; 2],
    rlp_value: [RLPValueGadget<F>; 2],
    drifted_rlp_key: LeafKeyGadget<F>,
    wrong_rlp_key: LeafKeyGadget<F>,
    is_wrong_leaf: Cell<F>,
    check_is_wrong_leaf: RequireNotZeroGadget<F>,
    is_not_hashed: [LtGadget<F, 1>; 2],
    is_empty_trie: [IsEmptyTreeGadget<F>; 2],
}

impl<F: Field> StorageLeafConfig<F> {
    pub fn configure(
        meta: &mut VirtualCells<'_, F>,
        cb: &mut MPTConstraintBuilder<F>,
        ctx: MPTContext<F>,
    ) -> Self {
        let r = ctx.r.clone();

        cb.base.cell_manager.as_mut().unwrap().reset();
        let mut config = StorageLeafConfig::default();

        circuit!([meta, cb.base], {
            let key_bytes = [
                ctx.expr(meta, 0)[..36].to_owned(),
                ctx.expr(meta, 2)[..36].to_owned(),
            ];
            let value_bytes = [ctx.expr(meta, 1), ctx.expr(meta, 3)];
            let drifted_bytes = ctx.expr(meta, 4)[..36].to_owned();
            let wrong_bytes = ctx.expr(meta, 5)[..36].to_owned();
            let lookup_offset = 3;
            let wrong_offset = 5;

            let mut key_rlc = vec![0.expr(); 2];
            let mut value_rlc = vec![0.expr(); 2];
            let mut value_rlp_rlc = vec![0.expr(); 2];
            for is_s in [true, false] {
                // Parent data
                let parent_data = &mut config.parent_data[is_s.idx()];
                *parent_data = ParentData::load(
                    "leaf load",
                    &mut cb.base,
                    &ctx.memory[parent_memory(is_s)],
                    0.expr(),
                );
                // Key data
                let key_data = &mut config.key_data[is_s.idx()];
                *key_data = KeyData::load(&mut cb.base, &ctx.memory[key_memory(is_s)], 0.expr());

                // Placeholder leaf checks
                config.is_empty_trie[is_s.idx()] =
                    IsEmptyTreeGadget::construct(&mut cb.base, parent_data.rlc.expr(), &r);
                let is_placeholder_leaf = config.is_empty_trie[is_s.idx()].expr();

                let rlp_key = &mut config.rlp_key[is_s.idx()];
                *rlp_key = LeafKeyGadget::construct(&mut cb.base, &key_bytes[is_s.idx()]);
                config.rlp_value[is_s.idx()] =
                    RLPValueGadget::construct(&mut cb.base, &value_bytes[is_s.idx()][0..36]);

                config.key_mult[is_s.idx()] = cb.base.query_cell();
                require!((FixedTableTag::RMult, rlp_key.num_bytes_on_key_row(), config.key_mult[is_s.idx()].expr()) => @"fixed");

                // RLC bytes zero check
                //cb.set_length(rlp_key.num_bytes_on_key_row());
                //cb.set_length_s(config.rlp_value[is_s.idx()].num_bytes());

                (value_rlc[is_s.idx()], value_rlp_rlc[is_s.idx()]) =
                    config.rlp_value[is_s.idx()].rlc(&r);

                let leaf_rlc = (rlp_key.rlc(&r), config.key_mult[is_s.idx()].expr())
                    .rlc_chain(value_rlp_rlc[is_s.idx()].expr());

                // Key
                key_rlc[is_s.idx()] = key_data.rlc.expr()
                    + rlp_key.leaf_key_rlc(
                        &mut cb.base,
                        key_data.mult.expr(),
                        key_data.is_odd.expr(),
                        1.expr(),
                        true,
                        &r,
                    );
                // Total number of nibbles needs to be KEY_LEN_IN_NIBBLES (except in a
                // placeholder leaf).
                // TODO(Brecht): why not in placeholder leaf?
                ifx! {not!(is_placeholder_leaf) => {
                    let num_nibbles = rlp_key.num_key_nibbles(key_data.is_odd.expr());
                    require!(key_data.num_nibbles.expr() + num_nibbles => KEY_LEN_IN_NIBBLES);
                }}

                // If `is_modified_node_empty = 1`, which means an empty child, we need to
                // ensure that the value is set to 0 in the placeholder leaf. For
                // example when adding a new storage leaf to the trie, we have an empty child in
                // `S` proof and non-empty in `C` proof.
                ifx! {is_placeholder_leaf => {
                    require!(value_rlc[is_s.idx()] => 0);
                }}

                // Make sure the RLP encoding is correct.
                // storage = [key, value]
                ifx! {not!(is_placeholder_leaf) => {
                    require!(rlp_key.num_bytes() => rlp_key.num_bytes_on_key_row() + config.rlp_value[is_s.idx()].num_bytes());
                }};

                // Check if the account is in its parent.
                // Check is skipped for placeholder leafs which are dummy leafs
                ifx! {not!(is_placeholder_leaf) => {
                    config.is_not_hashed[is_s.idx()] = LtGadget::construct(&mut cb.base, rlp_key.num_bytes(), 32.expr());
                    ifx!{or::expr(&[parent_data.is_root.expr(), not!(config.is_not_hashed[is_s.idx()])]) => {
                        // Hashed branch hash in parent branch
                        require!((1, leaf_rlc, rlp_key.num_bytes(), parent_data.rlc) => @"keccak");
                    } elsex {
                        // Non-hashed branch hash in parent branch
                        require!(leaf_rlc => parent_data.rlc);
                    }}
                }}

                // Key done, set the default values
                KeyData::store(
                    &mut cb.base,
                    &ctx.memory[key_memory(is_s)],
                    KeyData::default_values(),
                );
                // Store the new parent
                ParentData::store(
                    &mut cb.base,
                    &ctx.memory[parent_memory(is_s)],
                    [0.expr(), true.expr(), false.expr(), 0.expr()],
                );
            }

            // Drifted leaf
            ifx! {config.parent_data[true.idx()].is_placeholder.expr() + config.parent_data[false.idx()].is_placeholder.expr() => {
                config.drifted_rlp_key = LeafKeyGadget::construct(&mut cb.base, &drifted_bytes);

                // We need the intermediate key RLC right before `drifted_index` is added to it.
                let (key_rlc_prev, key_mult_prev, placeholder_nibble, placeholder_is_odd) = ifx!{config.parent_data[true.idx()].is_placeholder.expr() => {
                    (
                        config.key_data[true.idx()].parent_rlc.expr(),
                        config.key_data[true.idx()].parent_mult.expr(),
                        config.key_data[true.idx()].placeholder_nibble.expr(),
                        config.key_data[true.idx()].placeholder_is_odd.expr(),
                    )
                } elsex {
                    (
                        config.key_data[false.idx()].parent_rlc.expr(),
                        config.key_data[false.idx()].parent_mult.expr(),
                        config.key_data[false.idx()].placeholder_nibble.expr(),
                        config.key_data[false.idx()].placeholder_is_odd.expr(),
                    )
                }};

                // Calculate the drifted key RLC
                let drifted_key_rlc = key_rlc_prev.expr() +
                    drifted_nibble_rlc(&mut cb.base, placeholder_nibble.expr(), key_mult_prev.expr(), placeholder_is_odd.expr()) +
                    config.drifted_rlp_key.leaf_key_rlc(&mut cb.base, key_mult_prev.expr(), placeholder_is_odd.expr(), r[0].expr(), true, &r);

                // Check zero bytes and mult_diff
                config.drifted_mult = cb.base.query_cell();
                require!((FixedTableTag::RMult, config.drifted_rlp_key.num_bytes_on_key_row(), config.drifted_mult.expr()) => @"fixed");
                cb.set_length(config.drifted_rlp_key.num_bytes_on_key_row());

                // Check that the drifted leaf is unchanged and is stored at `drifted_index`.
                let calc_rlc = |is_s: bool| {
                    // Complete the drifted leaf rlc by adding the bytes on the value row
                    let drifted_rlc = (config.drifted_rlp_key.rlc(&r), config.drifted_mult.expr()).rlc_chain(value_rlp_rlc[is_s.idx()].expr());
                    (key_rlc[is_s.idx()].expr(), drifted_rlc, config.parent_data[is_s.idx()].placeholder_rlc.expr())
                };
                let (key_rlc, drifted_rlc, mod_hash) = matchx! {
                    config.parent_data[true.idx()].is_placeholder => {
                        // Neighbour leaf in the added branch
                        calc_rlc(true)
                    },
                    config.parent_data[false.idx()].is_placeholder => {
                        // Neighbour leaf in the deleted branch
                        calc_rlc(false)
                    },
                };
                // The key of the drifted leaf needs to match the key of the leaf
                require!(key_rlc => drifted_key_rlc);
                // The drifted leaf needs to be stored in the branch at `drifted_index`.
                require!((1, drifted_rlc, config.drifted_rlp_key.num_bytes(), mod_hash) => @"keccak");
            }}

            // Wrong leaf
            config.is_wrong_leaf = cb.base.query_cell();
            // Make sure is_wrong_leaf is boolean
            require!(config.is_wrong_leaf => bool);
            ifx! {a!(ctx.proof_type.is_non_existing_storage_proof, wrong_offset) => {
                // Get the previous key data
                config.key_data_w = KeyData::load(&mut cb.base, &ctx.memory[key_memory(true)], 1.expr());
                ifx! {config.is_wrong_leaf => {
                    // Calculate the key
                    config.wrong_rlp_key = LeafKeyGadget::construct(&mut cb.base, &wrong_bytes);
                    let key_rlc_wrong = config.key_data_w.rlc.expr() + config.wrong_rlp_key.leaf_key_rlc(
                        &mut cb.base,
                        config.key_data_w.mult.expr(),
                        config.key_data_w.is_odd.expr(),
                        1.expr(),
                        false,
                        &ctx.r,
                    );

                    // Check that it's the key as requested in the lookup
                    let key_rlc_lookup = a!(ctx.mpt_table.key_rlc, wrong_offset);
                    require!(key_rlc_lookup => key_rlc_wrong);

                    // Now make sure this address is different than the one of the leaf
                    config.check_is_wrong_leaf = RequireNotZeroGadget::construct(&mut cb.base, key_rlc_lookup - key_rlc[false.idx()].expr());
                    // Make sure the lengths of the keys are the same
                    require!(config.wrong_rlp_key.key_len() => config.rlp_key[false.idx()].key_len());
                    // RLC bytes zero check
                    cb.set_length(config.wrong_rlp_key.num_bytes_on_key_row());
                } elsex {
                    // In case when there is no wrong leaf, we need to check there is a nil object in the parent branch.
                    require!(config.key_data_w.is_placeholder_leaf_c => true);
                }}
            } elsex {
                // is_wrong_leaf needs to be false when not in non_existing_account proof
                require!(config.is_wrong_leaf => false);
            }}

            // Put the data in the lookup table
            require!(a!(ctx.mpt_table.key_rlc, lookup_offset) => key_rlc[false.idx()]);
            require!(a!(ctx.mpt_table.value_prev, lookup_offset) => value_rlc[true.idx()]);
            require!(a!(ctx.mpt_table.value, lookup_offset) => value_rlc[false.idx()]);
        });

        config
    }

    pub fn assign(
        &self,
        region: &mut Region<'_, F>,
        ctx: &MPTConfig<F>,
        witness: &[MptWitnessRow<F>],
        pv: &mut ProofValues<F>,
        offset: usize,
    ) -> Result<(), Error> {
        let base_offset = offset;

        let key_bytes = [&witness[offset + 0], &witness[offset + 2]];
        let value_bytes = [&witness[offset + 1], &witness[offset + 3]];
        let drifted_bytes = &witness[offset + 4];
        let wrong_bytes = &witness[offset + 5];
        let lookup_offset = offset + 3;
        let wrong_offset = offset + 5;

        let mut key_rlc = vec![0.scalar(); 2];
        let mut value_rlc = vec![0.scalar(); 2];
        for is_s in [true, false] {
            // Key
            let key_row = &key_bytes[is_s.idx()];

            let rlp_key_witness =
                self.rlp_key[is_s.idx()].assign(region, base_offset, &key_row.bytes)?;

            let (_, leaf_mult) = rlp_key_witness.rlc_leaf(ctx.randomness);
            self.key_mult[is_s.idx()].assign(region, base_offset, leaf_mult)?;

            self.is_not_hashed[is_s.idx()].assign(
                region,
                base_offset,
                rlp_key_witness.num_bytes().scalar(),
                32.scalar(),
            )?;

            self.key_data[is_s.idx()].witness_load(
                region,
                base_offset,
                &mut pv.memory[key_memory(is_s)],
                0,
            )?;
            self.key_data[is_s.idx()].witness_store(
                region,
                base_offset,
                &mut pv.memory[key_memory(is_s)],
                F::zero(),
                F::one(),
                0,
                false,
                false,
                0,
                false,
                F::zero(),
                F::one(),
            )?;

            // For leaf S and leaf C we need to start with the same rlc.
            let mut key_rlc_new = pv.key_rlc;
            let mut key_rlc_mult_new = pv.key_rlc_mult;
            if (pv.is_branch_s_placeholder
                && key_row.get_type() == MptWitnessRowType::StorageLeafSKey)
                || (pv.is_branch_c_placeholder
                    && key_row.get_type() == MptWitnessRowType::StorageLeafCKey)
            {
                key_rlc_new = pv.key_rlc_prev;
                key_rlc_mult_new = pv.key_rlc_mult_prev;
            }
            (key_rlc[is_s.idx()], _) =
                rlp_key_witness.leaf_key_rlc(key_rlc_new, key_rlc_mult_new, ctx.randomness);

            // Value
            let value_row = &value_bytes[is_s.idx()];

            let value_witness =
                self.rlp_value[is_s.idx()].assign(region, base_offset, &value_row.bytes)?;

            value_rlc[is_s.idx()] = value_row.bytes
                [value_witness.num_rlp_bytes() as usize..HASH_WIDTH + 2]
                .rlc_value(ctx.randomness);

            let parent_values = self.parent_data[is_s.idx()].witness_load(
                region,
                base_offset,
                &mut pv.memory[parent_memory(is_s)],
                0,
            )?;
            self.parent_data[is_s.idx()].witness_store(
                region,
                base_offset,
                &mut pv.memory[parent_memory(is_s)],
                F::zero(),
                true,
                false,
                F::zero(),
            )?;

            self.is_empty_trie[is_s.idx()].assign(
                region,
                base_offset,
                parent_values[0],
                ctx.randomness,
            )?;
        }

        // Drifted leaf handling
        if pv.is_branch_s_placeholder || pv.is_branch_c_placeholder {
            let row = &drifted_bytes;

            let drifted_key_witness =
                self.drifted_rlp_key
                    .assign(region, base_offset, &row.bytes)?;

            let (_, leaf_mult) = drifted_key_witness.rlc_leaf(ctx.randomness);

            self.drifted_mult.assign(region, base_offset, leaf_mult)?;
        }

        // Non-existing
        let row = &wrong_bytes;
        if row.get_byte_rev(IS_NON_EXISTING_STORAGE_POS) == 1 {
            self.key_data_w.witness_load(
                region,
                base_offset,
                &mut pv.memory[key_memory(true)],
                1,
            )?;

            // TODO(Brecht): Change how the witness is generated
            let is_wrong = row.bytes[0] != 0;
            self.is_wrong_leaf
                .assign(region, base_offset, F::from(is_wrong))?;

            let mut row_bytes = row.bytes.clone();
            row_bytes[0] = key_bytes[false.idx()].bytes[0];

            let wrong_witness = self.wrong_rlp_key.assign(region, base_offset, &row_bytes)?;
            let (key_rlc_wrong, _) =
                wrong_witness.leaf_key_rlc(pv.key_rlc, pv.key_rlc_mult, ctx.randomness);

            self.check_is_wrong_leaf.assign(
                region,
                base_offset,
                key_rlc_wrong - key_rlc[false.idx()],
            )?;

            assign!(region, (ctx.mpt_table.key_rlc, wrong_offset) => key_rlc_wrong)?;
            assign!(region, (ctx.proof_type.proof_type, wrong_offset) => ProofType::StorageDoesNotExist.scalar())?;
        }

        // Put the data in the lookup table
        if value_bytes[false.idx()].get_byte_rev(IS_STORAGE_MOD_POS) == 1 {
            assign!(region, (ctx.proof_type.proof_type, lookup_offset) => ProofType::StorageChanged.scalar())?;
        }
        assign!(region, (ctx.mpt_table.key_rlc, lookup_offset) => key_rlc[false.idx()])?;
        assign!(region, (ctx.mpt_table.value_prev, lookup_offset) => value_rlc[true.idx()])?;
        assign!(region, (ctx.mpt_table.value, lookup_offset) => value_rlc[false.idx()])?;

        Ok(())
    }
}
