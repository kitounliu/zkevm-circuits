use eth_types::Field;
use gadgets::util::Scalar;
use halo2_proofs::plonk::{Error, VirtualCells};

use super::{
    helpers::{ListKeyGadget, MPTConstraintBuilder, ListKeyWitness},
    rlp_gadgets::RLPItemWitness,
    MPTContext,
};
use crate::{
    circuit,
    circuit_tools::{
        cached_region::CachedRegion, cell_manager::Cell, constraint_builder::RLCChainableRev,
        gadgets::LtGadget,
    },
    mpt_circuit::{
        helpers::{
            Indexable, ParentData, KECCAK, parent_memory, FIXED,
        },
        RlpItemType, witness_row::StorageRowType, FixedTableTag, param::HASH_WIDTH,
    },
};

#[derive(Clone, Debug, Default)]
pub(crate) struct ModExtensionGadget<F> {
    rlp_key: [ListKeyGadget<F>; 2],
    is_not_hashed: [LtGadget<F, 2>; 2],
    is_key_part_odd: [Cell<F>; 2],
    mult_key: Cell<F>,
}

impl<F: Field> ModExtensionGadget<F> {
    pub fn configure(
        meta: &mut VirtualCells<'_, F>,
        cb: &mut MPTConstraintBuilder<F>,
        ctx: MPTContext<F>,
        parent_data: &mut [ParentData<F>; 2],
    ) -> Self {
        let mut config = ModExtensionGadget::default();

        circuit!([meta, cb], {
            let key_items = [
                ctx.rlp_item(
                    meta,
                    cb,
                    StorageRowType::LongExtNodeKey as usize,
                    RlpItemType::Key,
                ),
                ctx.rlp_item(
                    meta,
                    cb,
                    StorageRowType::ShortExtNodeKey as usize,
                    RlpItemType::Key,
                ),
            ];
            let rlp_value = [
                ctx.rlp_item(
                    meta,
                    cb,
                    StorageRowType::LongExtNodeValue as usize,
                    RlpItemType::Node,
                ),
                ctx.rlp_item(
                    meta,
                    cb,
                    StorageRowType::ShortExtNodeValue as usize,
                    RlpItemType::Node,
                ),
            ];

            for is_s in [true, false] {
                config.rlp_key[is_s.idx()] = ListKeyGadget::construct(cb, &key_items[is_s.idx()]);
                config.is_key_part_odd[is_s.idx()] = cb.query_cell();

                let first_byte = matchx! {
                    key_items[is_s.idx()].is_short() => key_items[is_s.idx()].bytes_be()[0].expr(),
                    key_items[is_s.idx()].is_long() => key_items[is_s.idx()].bytes_be()[1].expr(),
                    key_items[is_s.idx()].is_very_long() => key_items[is_s.idx()].bytes_be()[2].expr(),
                };
                require!((FixedTableTag::ExtOddKey.expr(),
                    first_byte, config.is_key_part_odd[is_s.idx()].expr()) => @FIXED);
                
                // RLP encoding checks: [key, branch]
                // Verify that the lengths are consistent.
                require!(config.rlp_key[is_s.idx()].rlp_list.len() => config.rlp_key[is_s.idx()].key_value.num_bytes() + rlp_value[is_s.idx()].num_bytes());

                // Extension node RLC
                let node_rlc = config
                    .rlp_key[0]
                    .rlc2(&cb.keccak_r)
                    .rlc_chain_rev(rlp_value[is_s.idx()].rlc_chain_data());

                config.is_not_hashed[is_s.idx()] = LtGadget::construct(
                    &mut cb.base,
                    config.rlp_key[is_s.idx()].rlp_list.num_bytes(),
                    HASH_WIDTH.expr(),
                );

                let (rlc, num_bytes, is_not_hashed) =  
                    (
                        node_rlc.expr(),
                        config.rlp_key[is_s.idx()].rlp_list.num_bytes(),
                        config.is_not_hashed[is_s.idx()].expr(),
                    );

                 
                if is_s {
                    let parent_data = &mut parent_data[is_s.idx()];
                    *parent_data =
                    ParentData::load("leaf load", cb, &ctx.memory[parent_memory(is_s)], 0.expr());

                    ifx!{or::expr(&[parent_data.is_root.expr(), not!(is_not_hashed)]) => {
                        // Hashed branch hash in parent branch
                        require!(vec![1.expr(), rlc.expr(), num_bytes.expr(), parent_data.hash.lo().expr(), parent_data.hash.hi().expr()] => @KECCAK);
                    } elsex {
                        // Non-hashed branch hash in parent branch
                        require!(rlc => parent_data.rlc);
                    }}
                }

            }
            

            // TODO:
        });

        config
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assign(
        &self,
        region: &mut CachedRegion<'_, '_, F>,
        offset: usize,
        rlp_values: &[RLPItemWitness],
        list_rlp_bytes: [Vec<u8>; 2],
        // is_key_odd: &mut bool,
    ) -> Result<(), Error> {
        let key_items = [
            rlp_values[StorageRowType::LongExtNodeKey as usize].clone(),
            rlp_values[StorageRowType::ShortExtNodeKey as usize].clone(),
        ];
        let value_bytes = [
            rlp_values[StorageRowType::LongExtNodeValue as usize].clone(),
            rlp_values[StorageRowType::ShortExtNodeValue as usize].clone(),
        ];

        let mut rlp_key = vec![ListKeyWitness::default(); 2];

        for is_s in [true, false] {
            rlp_key[is_s.idx()] = self.rlp_key[is_s.idx()].assign(
                region,
                offset,
                &list_rlp_bytes[is_s.idx()],
                &key_items[is_s.idx()],
            )?;

            let first_key_byte =
                key_items[is_s.idx()].bytes[rlp_key[is_s.idx()].key_item.num_rlp_bytes()];

            let is_key_part_odd = first_key_byte >> 4 == 1;
            if is_key_part_odd {
                assert!(first_key_byte < 0b10_0000);
            } else {
                assert!(first_key_byte == 0);
            }

            self.is_key_part_odd[is_s.idx()]
            .assign(region, offset, is_key_part_odd.scalar())?;

            self.is_not_hashed[is_s.idx()].assign(
                region,
                offset,
                rlp_key[is_s.idx()].rlp_list.num_bytes().scalar(),
                HASH_WIDTH.scalar(),
            )?;

            /*
            let mut key_len_mult = rlp_key[is_s.idx()].key_item.len();
            if !(*is_key_odd[is_s.idx()] && is_key_part_odd) {
                key_len_mult -= 1;
            }

            // Update number of nibbles
            // *num_nibbles += num_nibbles::value(rlp_key.key_item.len(), is_key_part_odd);

            // Update parity
            *is_key_odd = if is_key_part_odd {
                !*is_key_odd
            } else {
                *is_key_odd
            };

            // Key RLC
            let (key_rlc_ext, _) = ext_key_rlc_calc_value(
                rlp_key.key_item,
                key_data.mult,
                is_key_part_odd,
                !*is_key_odd,
                key_items
                    .iter()
                    .map(|item| item.bytes.clone())
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                region.key_r,
            );
            *key_rlc = key_data.rlc + key_rlc_ext;

            // Key mult
            let mult_key = pow::value(region.key_r, key_len_mult);
            self.mult_key.assign(region, offset, mult_key)?;
            // *key_mult = key_data.mult * mult_key;
            */

        }
        
        // TODO

        Ok(())
    }
}
