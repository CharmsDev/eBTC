use charms_sdk::data::{App, Data, Transaction};

pub fn app_contract(_app: &App, _tx: &Transaction, _x: &Data, _w: &Data) -> bool {
    false
}
