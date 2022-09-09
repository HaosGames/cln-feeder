use cln_rpc::model::*;
use cln_rpc::primitives::ShortChannelId;
use cln_rpc::ClnRpc;
use log::debug;
use std::collections::HashMap;

pub async fn get_revenue_since(
    last_updated: i64,
    short_channel_id: ShortChannelId,
    client: &mut ClnRpc,
) -> u64 {
    let mut revenue = 0;
    if let Response::ListForwards(forwards) = client
        .call(Request::ListForwards(ListforwardsRequest {
            status: Some(ListforwardsStatus::SETTLED),
            in_channel: None,
            out_channel: Some(short_channel_id),
        }))
        .await
        .expect("Couldn't get current revenue")
    {
        for payment in forwards.forwards {
            if payment.received_time > last_updated as f64 {
                revenue += payment.fee_msat.unwrap().msat();
            }
        }
    }
    revenue
}
pub async fn get_current_peers(client: &mut ClnRpc) -> Vec<ListpeersPeers> {
    if let Response::ListPeers(peers) = client
        .call(Request::ListPeers(ListpeersRequest {
            id: None,
            level: None,
        }))
        .await
        .expect("Couldn't get peers")
    {
        peers.peers
    } else {
        vec![]
    }
}
pub async fn get_current_fees(client: &mut ClnRpc) -> HashMap<String, u32> {
    let mut fees = HashMap::new();
    for peer in get_current_peers(client).await {
        if !peer.connected {
            continue;
        }
        for channel in peer.channels {
            if let ListpeersPeersChannelsState::CHANNELD_NORMAL = channel.state {
                fees.insert(
                    channel.short_channel_id.unwrap().to_string(),
                    channel.fee_proportional_millionths.unwrap(),
                );
            }
        }
    }
    fees
}
pub async fn set_channel_fee(client: &mut ClnRpc, channel: &String, fee: u32) {
    client
        .call(Request::SetChannel(SetChannelRequest {
            id: channel.clone(),
            feebase: None,
            feeppm: Some(fee),
            htlcmin_masat: None,
            htlcmax_msat: None,
        }))
        .await
        .expect("Couldn't set new fee");
    debug!("Set fee {} msats for {}", fee, channel);
}
