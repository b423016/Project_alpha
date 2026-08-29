use neural_router_domain::NewOrder;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderState {
    Submitted,
    Partial,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlotterRow {
    pub client_order_id: String,
    pub occ: String,
    pub qty: u32,
    pub filled_qty: u32,
    pub state: OrderState,
}

#[derive(Debug, Default)]
pub struct Blotter {
    pub rows: Vec<BlotterRow>,
}

impl Blotter {
    pub fn insert_submitted(&mut self, order: &NewOrder) {
        if self
            .rows
            .iter()
            .any(|r| r.client_order_id == order.client_order_id)
        {
            return;
        }
        self.rows.push(BlotterRow {
            client_order_id: order.client_order_id.clone(),
            occ: order.occ.as_str().into(),
            qty: order.qty,
            filled_qty: 0,
            state: OrderState::Submitted,
        });
    }

    pub fn filled_qty(&self, occ: &str) -> i64 {
        self.rows
            .iter()
            .filter(|r| r.occ == occ)
            .map(|r| i64::from(r.filled_qty))
            .sum()
    }
}
