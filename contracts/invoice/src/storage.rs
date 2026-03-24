use soroban_sdk::{contracttype, Address, Env, String};

/// Represents the lifecycle state of an invoice.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum InvoiceStatus {
    /// Invoice created, awaiting client funding.
    Pending,
    /// Client has deposited funds into escrow.
    Funded,
    /// Freelancer has marked work as delivered.
    Delivered,
    /// Client has approved the delivery.
    Approved,
    /// Funds have been released to the freelancer.
    Completed,
    // TODO: Consider adding Disputed and Cancelled states
    // See: https://github.com/your-org/StarInvoice/issues/5
}

/// Core invoice data structure stored on-chain.
#[contracttype]
#[derive(Clone)]
pub struct Invoice {
    /// Unique numeric identifier for this invoice.
    pub id: u64,
    /// Address of the freelancer who created the invoice.
    pub freelancer: Address,
    /// Address of the client responsible for funding.
    pub client: Address,
    /// Payment amount in the smallest token unit (stroops).
    pub amount: i128,
    /// Human-readable description of the work to be performed.
    pub description: String,
    /// Current state of the invoice in the escrow lifecycle.
    pub status: InvoiceStatus,
    // TODO: Add deadline / expiry field
    // TODO: Add token address field for multi-token support
    // See: https://github.com/your-org/StarInvoice/issues/6
}

#[contracttype]
enum DataKey {
    Invoice(u64),
    InvoiceCount,
}

/// Returns the next available invoice ID and increments the counter.
pub fn next_invoice_id(env: &Env) -> u64 {
    let count: u64 = env
        .storage()
        .instance()
        .get(&DataKey::InvoiceCount)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::InvoiceCount, &(count + 1));
    count
}

/// Persists an invoice to on-chain storage, keyed by its ID.
pub fn save_invoice(env: &Env, invoice: &Invoice) {
    env.storage()
        .persistent()
        .set(&DataKey::Invoice(invoice.id), invoice);
}

/// Retrieves an invoice by ID. Panics if the invoice does not exist.
pub fn get_invoice(env: &Env, invoice_id: u64) -> Invoice {
    env.storage()
        .persistent()
        .get(&DataKey::Invoice(invoice_id))
        .expect("Invoice not found")
}
