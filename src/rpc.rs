use crate::ITERATION_SECONDS;
use chrono::{Duration, Utc};
use cln_rpc::model::*;
use cln_rpc::primitives::ShortChannelId;
use cln_rpc::ClnRpc;
use std::collections::HashMap;

pub async fn get_current_revenue(short_channel_id: ShortChannelId, client: &mut ClnRpc) -> u64 {
    let last_updated =
        (Utc::now() + Duration::seconds(ITERATION_SECONDS.into())).timestamp() as f64;
    let mut revenue = 0;
    if let Response::ListForwards(forwards) = client
        .call(Request::ListForwards(ListforwardsRequest {
            status: Some(ListforwardsStatus::SETTLED),
            in_channel: None,
            out_channel: Some(short_channel_id),
        }))
        .await
        .unwrap()
    {
        for payment in forwards.forwards {
            if payment.received_time > last_updated {
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
        .unwrap()
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
