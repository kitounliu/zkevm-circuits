use ethers::providers::Middleware;
use ethers::types::transaction::eip2930::{AccessList, AccessListItem};
use ethers::{
    prelude::*,
};
use eyre::Result;
use std::str::FromStr;

use crate::transforms::Transforms;

mod circuit;
mod contract;
mod transforms;
mod utils;



#[tokio::main]
async fn main() -> Result<()> {
    const PVK: &str = "7ccb34dc5fd31fd0aa7860de89a4adc37ccb34dc5fd31fd0aa7860de89a4adc3";
    const PROVIDER_URL: &str = "https://mainnet.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161";

    let client = utils::new_eth_client(PROVIDER_URL, PVK).await?;

    let tests: Vec< ( u64 ,  Vec<(&str, Vec<&str>)>) > = vec![
        // 0 txs
        ( 107, vec![("0xd7E30ae310C1D1800F5B641Baa7af95b2e1FD98C", vec![])] ),
        // 4 transfer txs
        ( 436875, vec![
            ("0x580992B51e3925e23280EfB93d3047C82f17E038", vec![]),
            ("0x52bc44d5378309EE2abF1539BF71dE1b7d7bE3b5", vec![]),
            ("0x15ac3b6F90549FFBE4091177B1795B3d4C11A59e", vec![]),
            ("0x72382223a91051A54c69759BE3c93048235EfC43", vec![])
            ]),
        // TheDAO, 4 storage changes
        ( 2000007, vec![
            ("0x61C808D82A3Ac53231750daDc13c777b59310bD9", vec![]), // coinbase
            ("0xDf21fA922215B1a56f5a6D6294E6E36c85A0Acfb", vec![]), // tx1 from
            ("0xBB9bc244D798123fDe783fCc1C72d3Bb8C189413", vec![
                "0x0db619cb4b09b98626d1a90813a5566d6ae59d0a68df3e729f07a4cf6a7169fe",
                "0x28f0a29873c622b02659ae6083b0cf3fb2c44358fa1b0c0efb89893011b2cf8b",
                "0xf610712b7b4266f7fbc44538628f0ffdcb93c6d56f73a4dfeb041befdf2c9058",
                "0xf903a85392f66de7c382c130eb51940c4bfed2038df5c108d8c0115c24efcc94",
            ]), // tx1 to
        ]),

        // TheDAO. Storage does not exist
        ( 2000070, vec![
            ("0x1a060B0604883A99809eB3F798DF71BEf6c358f1", vec![]), // coinbase
            ("0xEd8387812f6477a421f2a16975a6121FC91933e6", vec![]), // tx1 from
            ("0xBB9bc244D798123fDe783fCc1C72d3Bb8C189413", vec![
                "0x4312ad16021fb135960665020d410e3ca0e42488b684d61315e73d368c7182ad",
                "0x83390858478ca0e9bd8e0b6f9c61cb360f78d42e5c5c2908d9a885b766925386",
            ]), // tx1 to
        ]),

        // A complex block
        ( 2000004, vec![
            ("0x4Bb96091Ee9D802ED039C4D1a5f6216F90f81B01", vec![]), // coinbase
            ("0x8975dBC1b8F25EC994815626D070899ddA896511", vec![]), // tx 0
            ("0xb2e3732C0B0eC387962f76fA4F1BB9325089C5E0", vec![]),
            ("0xeD059bc543141c8C93031d545079b3Da0233B27f", vec![]), // tx 1
            ("0xEc9F6c9634165f91e22E58B90e3EdE393d959E47", vec![]),
            ("0xcaac46d9bd68bffb533320545a90cd92c6e98e58", vec![
                "0x0000000000000000000000000000000000000000000000000000000000000000",
                "0x0000000000000000000000000000000000000000000000000000000000000001"
            ]),
            ("0xec9f6c9634165f91e22e58b90e3ede393d959e47", vec![
                "0x0000000000000000000000000000000000000000000000000000000000000003",
                "0x19da5b482fc1817af240c411d7423a456cdcf4a213e9f192aca5c80a39a4a733"
            ]),
            ("0xF05C1b271D12b7ECB3b37122730C085ec2C0B552", vec![]), // tx 2
            ("0x4FDf5371f7fFA04866f696882Db659fE38f52559", vec![]),
            ("0xBef52Af092Fa2349279f7A2B10779FE810785688", vec![]), // tx 3
            ("0x24F21c22F0e641e2371F04a7bB8d713f89f53550", vec![]),
        ])
    ];

    for (block_no, access_list) in tests {
        let access_list = AccessList(access_list.into_iter().map(|(addr, storage_keys)| {
            AccessListItem {
                address: Address::from_str(addr).unwrap(),
                storage_keys: storage_keys.into_iter().map(|k| H256::from_str(k).unwrap()).collect(),
            }
        }).collect());
        let trns = Transforms::new(client.clone(), U64::from(block_no), Some(access_list)).await?;
        println!("trns: {:#?}", trns);

        let witness = trns.witness(PROVIDER_URL)?;
        utils::verify_mpt_witness(witness)?;
    }

    Ok(())
}

async fn main_local() -> Result<()> {
    const PVK: &str = "7ccb34dc5fd31fd0aa7860de89a4adc37ccb34dc5fd31fd0aa7860de89a4adc3";
    const PROVIDER_URL: &str = "http://localhost:8545";


    let client = utils::new_eth_client(PROVIDER_URL, PVK).await?;
    let contract = contract::Contract::deploy(client.clone()).await?;
    let recipt = contract.set(0xcafee.into(), 0xcafe.into()).await?;
    let block_no = recipt.block_number.unwrap();
    let _tx = client.get_transaction(recipt.transaction_hash).await?.unwrap();

    let trns = Transforms::new(client.clone(), block_no, None).await?;
    println!("trns: {:#?}", trns);

    let witness = trns.witness(PROVIDER_URL)?;
    utils::verify_mpt_witness(witness)?;

    Ok(())
}