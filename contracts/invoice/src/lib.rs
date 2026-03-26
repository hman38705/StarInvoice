#! [no_std]

mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, token, Address, Env, String};

pub use storage::{Invoice, ContractError};

#[contract]
pub struct InvoiceContract;

#[contractimpl]
impl InvoiceContract {
    // ... all existing functions copy from previous read ...

    // Insert new fn before release_payment

    /// Allows the client to dispute the invoice from Funded or Delivered state.
    pub fn dispute_invoice(env: Env, invoice_id: u64) -> Result<(), ContractError> {
        let mut invoice = storage::get_invoice(&env, invoice_id)?;

        invoice.client.require_auth();

        if invoice.status == storage::InvoiceStatus::Pending {
            panic!("Cannot dispute pending invoice");
        }

        if invoice.status != storage::InvoiceStatus::Funded && invoice.status != storage::InvoiceStatus::Delivered {
            panic!("Can only dispute from Funded or Delivered status");
        }

        invoice.status = storage::InvoiceStatus::Disputed;
        storage::save_invoice(&env, &invoice);
        events::invoice_disputed(&env, invoice_id, &invoice.client);
        Ok(())
    }

    // existing release_payment ...

}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String};

    // all existing tests ...

    #[test]
    fn test_dispute_invoice_funded() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Dispute test");
        let amount: i128 = 1000;

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        let invoice_id = client.create_invoice(&freelancer, &payer, &amount, &description);
        client.fund_invoice(&invoice_id, &token_address);
        client.dispute_invoice(&invoice_id);

        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Disputed);
    }

    #[test]
    fn test_dispute_invoice_delivered() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Dispute test delivered");
        let amount: i128 = 1000;

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        let invoice_id = client.create_invoice(&freelancer, &payer, &amount, &description);
        client.fund_invoice(&invoice_id, &token_address);
        client.mark_delivered(&invoice_id);
        client.dispute_invoice(&invoice_id);

        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Disputed);
    }

    #[test]
    #[should_panic(expected = "Cannot dispute pending invoice")]
    fn test_dispute_invoice_pending() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Dispute test pending");

        let invoice_id = client.create_invoice(&freelancer, &payer, &100, &description);
        client.dispute_invoice(&invoice_id);
    }
}

