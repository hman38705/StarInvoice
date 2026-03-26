#![no_std]

mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, Address, Env, String};

pub use storage::Invoice;

#[contract]
pub struct InvoiceContract;

#[contractimpl]
impl InvoiceContract {
    /// Creates a new invoice and stores it on-chain.
    ///
    /// # Parameters
    /// - `freelancer`: Address of the service provider; must sign the transaction.
    /// - `client`: Address of the paying party.
    /// - `amount`: Payment amount in the smallest token unit (stroops). Uses `i128`;
    ///   overflow is prevented at the platform level via `overflow-checks = true`
    ///   in the `[profile.release]` section of `contracts/invoice/Cargo.toml`.
    /// - `description`: Human-readable description of the work.
    ///
    /// # Returns
    /// The newly assigned invoice ID.
    ///
    /// # Errors
    /// Panics if `freelancer` authorization fails.
    pub fn create_invoice(
        env: Env,
        freelancer: Address,
        client: Address,
        amount: i128,
        description: String,
    ) -> u64 {
        freelancer.require_auth();

        let invoice_id = storage::next_invoice_id(&env);

        let invoice = Invoice {
            id: invoice_id,
            freelancer: freelancer.clone(),
            client: client.clone(),
            amount,
            description,
            status: storage::InvoiceStatus::Pending,
        };

        storage::save_invoice(&env, &invoice);
        events::invoice_created(&env, invoice_id, &freelancer, &client, amount);

        invoice_id
    }

    /// Allows the client to deposit funds into escrow for the given invoice.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to fund.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice client.
    /// - Panics if the invoice status is not `Pending`.
    ///
    /// # TODO
    /// Not yet implemented. See: <https://github.com/your-org/StarInvoice/issues/1>
    pub fn fund_invoice(_env: Env, _invoice_id: u64) {
        todo!("fund_invoice not yet implemented")
    }

    /// Allows the freelancer to signal that work has been completed.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to mark as delivered.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice freelancer.
    /// - Panics if the invoice status is not `Funded`.
    ///
    /// # TODO
    /// Not yet implemented. See: <https://github.com/your-org/StarInvoice/issues/2>
    pub fn mark_delivered(_env: Env, _invoice_id: u64) {
        todo!("mark_delivered not yet implemented")
    }

    /// Allows the client to approve the delivered work, authorising fund release.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to approve.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice client.
    /// - Panics if the invoice status is not `Delivered`.
    ///
    /// # TODO
    /// Not yet implemented. See: <https://github.com/your-org/StarInvoice/issues/3>
    pub fn approve_payment(_env: Env, _invoice_id: u64) {
        todo!("approve_payment not yet implemented")
    }

    /// Releases escrowed funds to the freelancer once the invoice is approved.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to settle.
    ///
    /// # Errors
    /// - Panics if the invoice status is not `Approved`.
    ///
    /// # TODO
    /// Not yet implemented. See: <https://github.com/your-org/StarInvoice/issues/4>
    pub fn release_payment(_env: Env, _invoice_id: u64) {
        todo!("release_payment not yet implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String};

    #[test]
    fn test_create_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Website redesign - Phase 1");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        assert_eq!(invoice_id, 0);

        // Verify the invoice was stored correctly
        let invoice = storage::get_invoice(&env, invoice_id);
        assert_eq!(invoice.freelancer, freelancer);
        assert_eq!(invoice.client, payer);
        assert_eq!(invoice.amount, 1000);
    }

    #[test]
    fn test_create_invoice_max_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Max amount invoice");

        // Verify that i128::MAX is accepted without overflow.
        // Overflow protection is guaranteed by `overflow-checks = true` in
        // [profile.release] of contracts/invoice/Cargo.toml.
        let invoice_id = client.create_invoice(&freelancer, &payer, &i128::MAX, &description);

        let invoice = storage::get_invoice(&env, invoice_id);
        assert_eq!(invoice.amount, i128::MAX);
    }
}
