use soroban_sdk::{symbol_short, Address, Env};

/// Emits an event when a new invoice is created.
///
/// Topic: `("INVOICE", "created")`
/// Data:  `(invoice_id, freelancer, client, amount)`
pub fn invoice_created(
    env: &Env,
    invoice_id: u64,
    freelancer: &Address,
    client: &Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("INVOICE"), symbol_short!("created")),
        (invoice_id, freelancer.clone(), client.clone(), amount),
    );
}

/// Emits an event when a client approves delivered work.
///
/// Topic: `("INVOICE", "approved")`
/// Data:  `(invoice_id, client)`
pub fn approve_payment(env: &Env, invoice_id: u64, client: &Address) {
    env.events().publish(
        (symbol_short!("INVOICE"), symbol_short!("approved")),
        (invoice_id, client.clone()),
    );
}

// TODO: Add event emitters for each state transition:
// - fund_invoice    -> emit "INVOICE funded"   | data: (invoice_id, client)
// - mark_delivered  -> emit "INVOICE delivered" | data: (invoice_id, freelancer)
// - release_payment -> emit "INVOICE released"  | data: (invoice_id, amount)
// See: https://github.com/your-org/StarInvoice/issues/7
