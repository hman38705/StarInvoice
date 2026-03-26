#![no_std]

mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, token, Address, Env, String};

pub use storage::{Invoice, ContractError};

#[contract]
pub struct InvoiceContract;

#[contractimpl]
impl InvoiceContract {
    /// Creates a new invoice and stores it on-chain.
    ///
    /// # Parameters
    /// - `freelancer`: Address of the service provider; must sign the transaction.
    /// - `client`: Address of the paying party.
    /// - `amount`: Payment amount in the smallest token unit (stroops).
    /// - `token`: Address of the token contract used for payment.
    /// - `deadline`: Unix timestamp after which the invoice can no longer be funded.
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
        token: Address,
        deadline: u64,
        description: String,
    ) -> u64 {
        freelancer.require_auth();

        let invoice_id = storage::next_invoice_id(&env);

        let invoice = Invoice {
            id: invoice_id,
            freelancer: freelancer.clone(),
            client: client.clone(),
            amount,
            token,
            deadline,
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
    pub fn fund_invoice(env: Env, invoice_id: u64) {
        let mut invoice = storage::get_invoice(&env, invoice_id);

        invoice.client.require_auth();

        assert!(
            invoice.status == storage::InvoiceStatus::Pending,
            "Invoice must be in Pending status"
        );

        assert!(
            env.ledger().timestamp() <= invoice.deadline,
            "Invoice has expired"
        );

        let token = token::Client::new(&env, &invoice.token);
        token.transfer(&invoice.client, &env.current_contract_address(), &invoice.amount);

        invoice.status = storage::InvoiceStatus::Funded;
        storage::save_invoice(&env, &invoice);

        events::invoice_funded(&env, invoice_id, &invoice.client);
        Ok(())
    }

    /// Allows the freelancer to signal that work has been completed.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to mark as delivered.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice freelancer.
    /// - Panics if the invoice status is not `Funded`.
    pub fn mark_delivered(env: Env, invoice_id: u64) -> Result<(), ContractError> {
        let mut invoice = storage::get_invoice(&env, invoice_id)?;

        invoice.freelancer.require_auth();

        assert!(
            invoice.status == storage::InvoiceStatus::Funded,
            "Invoice must be in Funded status"
        );

        invoice.status = storage::InvoiceStatus::Delivered;
        storage::save_invoice(&env, &invoice);

        events::mark_delivered(&env, invoice_id, &invoice.freelancer);
        Ok(())
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
    pub fn approve_payment(env: Env, invoice_id: u64) -> Result<(), ContractError> {
        let mut invoice = storage::get_invoice(&env, invoice_id)?;

        invoice.client.require_auth();

        assert!(
            invoice.status == storage::InvoiceStatus::Delivered,
            "Invoice must be in Delivered status"
        );

        invoice.status = storage::InvoiceStatus::Approved;
        storage::save_invoice(&env, &invoice);

        events::approve_payment(&env, invoice_id, &invoice.client);
        Ok(())
    }

    /// Returns the current number of invoices.
    pub fn invoice_count(env: Env) -> u64 {
        storage::get_invoice_count(&env)
    }

    /// Returns the data for a specific invoice ID.
    pub fn get_invoice(env: Env, invoice_id: u64) -> Result<Invoice, ContractError> {
        storage::get_invoice(&env, invoice_id)
    }

    /// Cancels a Pending invoice, voiding it permanently.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to cancel.
    /// - `caller`: Address of the party requesting cancellation (freelancer or client).
    ///
    /// # Errors
    /// - Panics if the invoice status is not `Pending`.
    /// - Panics if `caller` is neither the freelancer nor the client.
    pub fn cancel_invoice(env: Env, invoice_id: u64, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();

        let mut invoice = storage::get_invoice(&env, invoice_id)?;

        assert!(
            invoice.status == storage::InvoiceStatus::Pending,
            "Invoice can only be cancelled from Pending status"
        );

        assert!(
            caller == invoice.freelancer || caller == invoice.client,
            "Only the freelancer or client can cancel the invoice"
        );

        invoice.status = storage::InvoiceStatus::Cancelled;
        storage::save_invoice(&env, &invoice);
        events::invoice_cancelled(&env, invoice_id, &caller);
        Ok(())
    }

    /// Releases escrowed funds to the freelancer once the invoice is approved.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to settle.
    ///
    /// # Errors
    /// - Panics if the invoice status is not `Approved`.
    pub fn release_payment(env: Env, invoice_id: u64) {
        let mut invoice = storage::get_invoice(&env, invoice_id);

        assert!(
            invoice.status == storage::InvoiceStatus::Approved,
            "Invoice must be in Approved status"
        );

        let token = token::Client::new(&env, &invoice.token);
        token.transfer(&env.current_contract_address(), &invoice.freelancer, &invoice.amount);

        invoice.status = storage::InvoiceStatus::Completed;
        storage::save_invoice(&env, &invoice);

        events::payment_released(&env, invoice_id, &invoice.freelancer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Env, String};

    fn setup_token(env: &Env) -> Address {
        let admin = Address::generate(env);
        env.register_stellar_asset_contract_v2(admin).address()
    }

    #[test]
    fn test_create_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "Website redesign - Phase 1");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &token_address, &9999999999, &description);

        assert_eq!(invoice_id, 0);

        // Verify the invoice was stored correctly
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.freelancer, freelancer);
        assert_eq!(invoice.client, payer);
        assert_eq!(invoice.amount, 1000);
        assert_eq!(invoice.token, token_address);
        assert_eq!(invoice.deadline, 9999999999);
    }

    #[test]
    fn test_cancel_invoice_by_freelancer() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "Logo design");

        let invoice_id = client.create_invoice(&freelancer, &payer, &500, &token_address, &9999999999, &description);
        client.cancel_invoice(&invoice_id, &freelancer);

        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Cancelled);
    }

    #[test]
    fn test_cancel_invoice_by_client() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "SEO audit");

        let invoice_id = client.create_invoice(&freelancer, &payer, &200, &token_address, &9999999999, &description);
        client.cancel_invoice(&invoice_id, &payer);

        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "Only the freelancer or client can cancel the invoice")]
    fn test_cancel_invoice_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let stranger = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "Branding package");

        let invoice_id = client.create_invoice(&freelancer, &payer, &750, &description);
        let _ = client.cancel_invoice(&invoice_id, &stranger);
    }

    #[test]
    #[should_panic(expected = "Invoice can only be cancelled from Pending status")]
    fn test_cancel_invoice_wrong_status() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client_contract = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "App development");

        let invoice_id = client_contract.create_invoice(&freelancer, &payer, &3000, &token_address, &9999999999, &description);
        client_contract.cancel_invoice(&invoice_id, &freelancer);

        // Attempt to cancel again — should panic
        let _ = client_contract.cancel_invoice(&invoice_id, &freelancer);
    }

    #[test]
    fn test_fund_invoice_happy_path() {
        use soroban_sdk::token;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let invoice_client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Smart contract audit");
        let amount: i128 = 5000;

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        // Set ledger timestamp before the deadline
        env.ledger().set_timestamp(1000);

        let invoice_id = invoice_client.create_invoice(&freelancer, &payer, &amount, &token_address, &9999999999, &description);
        invoice_client.fund_invoice(&invoice_id);

        // Assert status is now Funded
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Funded);

        let token_client = token::Client::new(&env, &token_address);
        assert_eq!(token_client.balance(&contract_id), amount);
        assert_eq!(token_client.balance(&payer), 0);
    }
    #[test]
    fn test_invoice_count() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        assert_eq!(client.invoice_count(), 0);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);

        client.create_invoice(
            &freelancer,
            &payer,
            &1000,
            &String::from_str(&env, "Desc 1"),
        );
        assert_eq!(client.invoice_count(), 1);

        client.create_invoice(
            &freelancer,
            &payer,
            &2000,
            &String::from_str(&env, "Desc 2"),
        );
        assert_eq!(client.invoice_count(), 2);
    }

    #[test]
    fn test_get_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test get_invoice");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);
        let invoice = client.get_invoice(&invoice_id);

        assert_eq!(invoice.id, invoice_id);
        assert_eq!(invoice.freelancer, freelancer);
        assert_eq!(invoice.client, payer);
        assert_eq!(invoice.amount, 1000);
        assert_eq!(invoice.description, description);
    }

    #[test]
    fn test_invoice_not_found() {
        let env = Env::default();
        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let result = client.try_get_invoice(&999);
        match result {
            Err(Ok(errors)) => {
                assert_eq!(errors, ContractError::InvoiceNotFound.into());
            }
            _ => panic!("Expected InvoiceNotFound error"),
        }
    }
}
