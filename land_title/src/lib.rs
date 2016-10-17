#![feature(type_ascription)]
#![feature(custom_derive)]
#![feature(plugin)]
#![plugin(serde_macros)]
#![feature(question_mark)]
#![feature(associated_consts)]

#[macro_use(message, storage_value)]
extern crate exonum;
extern crate serde;
extern crate byteorder;
extern crate blockchain_explorer;

mod txs;
mod view;
pub mod api;

use exonum::storage::{Map, Database, Error, List};
use exonum::blockchain::{Blockchain};
use exonum::messages::Message;
use exonum::crypto::{Hash, hash};
use std::u64;
use std::ops::Deref;

pub use txs::{ObjectTx, TxCreateOwner, TxCreateObject, TxModifyObject, Point,
              TxTransferObject, TxRemoveObject};
pub use view::{ObjectsView, Owner, Object, ObjectId, Ownership};

#[derive(Clone)]
pub struct ObjectsBlockchain<D: Database> {
    pub db: D,
}

impl<D: Database> Deref for ObjectsBlockchain<D> {
    type Target = D;

    fn deref(&self) -> &D {
        &self.db
    }
}

impl<D> Blockchain for ObjectsBlockchain<D> where D: Database
{
    type Database = D;
    type Transaction = ObjectTx;
    type View = ObjectsView<D::Fork>;

    fn verify_tx(tx: &Self::Transaction) -> bool {
        tx.verify(tx.pub_key())
    }

    fn state_hash(view: &Self::View) -> Result<Hash, Error> {
        let mut hashes = Vec::new();
        hashes.extend_from_slice(view.owners().root_hash()?.as_ref());
        hashes.extend_from_slice(view.objects().root_hash()?.as_ref());

        // for id in 0..view.owners().len()? as u64 {
        //     b.extend_from_slice(view.distributor_contracts(id).root_hash()?.as_ref())
        // }
        // for id in 0..view.objects().len()? as u64 {
        //     b.extend_from_slice(view.owner_contents(id).root_hash()?.as_ref())
        // }

        Ok(hash(&hashes))
    }

    fn execute(view: &Self::View, tx: &Self::Transaction) -> Result<(), Error> {

        match *tx {

            ObjectTx::CreateOwner(ref tx) => {
                if view.owners().get(tx.pub_key())?.is_some() {
                    return Err(Error::new(String::from("PublicKey already exists")));
                }
                let owner = Owner::new(tx.name(), &hash(&[]));
                view.owners().put(tx.pub_key(), owner)?;
            }

            ObjectTx::CreateObject(ref tx) => {
                let objects = view.objects();
                let object_id = objects.len()? as u64;

                // update ownership hash
                let owner_objects = view.owner_objects(tx.owner_pub_key());
                owner_objects.append(Ownership::new(object_id, true))?;
                let new_ownership_hash = owner_objects.root_hash()?;
                let mut owner = view.owners().get(tx.owner_pub_key())?.unwrap();
                owner.set_ownership_hash(&new_ownership_hash);
                view.owners().put(tx.owner_pub_key(), owner)?;

                // update object history hash
                let object_history = view.object_history(object_id);
                let hash = hash(&[]);
                object_history.append(hash)?;
                let new_history_hash = object_history.root_hash()?;

                // insert object
                let object = Object::new(tx.pub_key(), tx.title(), tx.points(), tx.owner_pub_key(), false, &new_history_hash);
                objects.append(object)?;

            }

            ObjectTx::ModifyObject(ref tx) => {

                if let Some(mut object) = view.objects().get(tx.object_id())? {
                    // update object history hash
                    let object_history = view.object_history(tx.object_id());
                    let hash = hash(&[]);
                    object_history.append(hash)?;
                    let new_history_hash = object_history.root_hash()?;

                    // update object
                    object.set_title(tx.title());
                    object.set_points(tx.points());
                    object.set_history_hash(&new_history_hash);
                    view.objects().set(tx.object_id(), object)?;
                }else{
                    return Err(Error::new(String::from("Object not found by id")));
                }

            }

            ObjectTx::TransferObject(ref tx) => {

                if let Some(mut object) = view.objects().get(tx.object_id())? {

                        // update ownership hash
                        let old_owner_objects = view.owner_objects(object.owner_pub_key());
                        old_owner_objects.append(Ownership::new(tx.object_id(), false))?;
                        let old_ownership_hash = old_owner_objects.root_hash()?;

                        let new_owner_objects = view.owner_objects(tx.owner_pub_key());
                        new_owner_objects.append(Ownership::new(tx.object_id(), true))?;
                        let new_ownership_hash = new_owner_objects.root_hash()?;

                        // update owners states
                        let mut old_owner = view.owners().get(object.owner_pub_key())?.unwrap();
                        old_owner.set_ownership_hash(&old_ownership_hash);
                        view.owners().put(object.owner_pub_key(), old_owner)?;

                        let mut new_owner = view.owners().get(tx.owner_pub_key())?.unwrap();
                        new_owner.set_ownership_hash(&new_ownership_hash);
                        view.owners().put(tx.owner_pub_key(), new_owner)?;

                        // update object history hash
                        let object_history = view.object_history(tx.object_id());
                        let hash = hash(&[]);
                        object_history.append(hash)?;
                        let new_history_hash = object_history.root_hash()?;

                        // update object
                        object.set_owner(tx.owner_pub_key());
                        object.set_history_hash(&new_history_hash);
                        view.objects().set(tx.object_id(), object)?;

                }else{
                    return Err(Error::new(String::from("Object not found by id")));
                }

            }

            ObjectTx::RemoveObject(ref tx) => {
                if let Some(mut object) = view.objects().get(tx.object_id())? {

                        // update object history hash
                        let object_history = view.object_history(tx.object_id());
                        let hash = hash(&[]);
                        object_history.append(hash)?;
                        let new_history_hash = object_history.root_hash()?;

                        // update object
                        object.set_deleted(true);
                        object.set_history_hash(&new_history_hash);
                        view.objects().set(tx.object_id(), object)?;

                }else{
                    return Err(Error::new(String::from("Object not found by id")));
                }
            }

        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::ObjectsBlockchain;
    use view::ObjectsView;
    use txs::{TxCreateOwner, TxCreateObject, TxModifyObject, TxTransferObject, TxRemoveObject, ObjectTx, Point};
    use exonum::crypto::{gen_keypair};
    use exonum::blockchain::Blockchain;
    use exonum::storage::{Map, List, Database, MemoryDB, Result as StorageResult};

    fn execute_tx<D: Database>(v: &ObjectsView<D::Fork>,
                               tx: ObjectTx)
                               -> StorageResult<()> {
        ObjectsBlockchain::<D>::execute(v, &tx)
    }

    #[test]
    fn test_add_owner() {

        // Arrange
        let b = ObjectsBlockchain { db: MemoryDB::new() };
        let v = b.view();
        let (p, s) = gen_keypair();
        let tx_create_owner = TxCreateOwner::new(&p, "test owner name", &s);

        // Act
        let ok_result = execute_tx::<MemoryDB>(&v, ObjectTx::CreateOwner(tx_create_owner.clone()));
        let err_result = execute_tx::<MemoryDB>(&v, ObjectTx::CreateOwner(tx_create_owner.clone()));
        let stored_owner = v.owners().get(&p).unwrap().unwrap();

        // Assert
        assert_eq!(ok_result.is_ok(), true);
        assert_eq!(err_result.is_ok(), false);
        assert_eq!(stored_owner.name(), "test owner name");

    }

    #[test]
    fn test_add_object(){

        // Arrange
        let b = ObjectsBlockchain { db: MemoryDB::new() };
        let v = b.view();
        let (p, s) = gen_keypair();
        let tx_create_owner = TxCreateOwner::new(&p, "test owner name", &s);
        let object_id = 0_u64;
        let points = [Point::new(1, 2).into(), Point::new(3, 4).into()];
        let tx_create_object = TxCreateObject::new(&p, "test object title", &points, &p, &s);

        // Act
        let create_owner_result = execute_tx::<MemoryDB>(&v, ObjectTx::CreateOwner(tx_create_owner.clone()));
        let create_object_result = execute_tx::<MemoryDB>(&v, ObjectTx::CreateObject(tx_create_object.clone()));
        let stored_object = v.objects().get(object_id).unwrap().unwrap();
        let stored_owner = v.owners().get(&p).unwrap().unwrap();
        let history_hash = v.object_history(object_id).root_hash().unwrap();
        let ownership_hash = v.owner_objects(&p).root_hash().unwrap();

        // Assert
        assert_eq!(create_owner_result.is_ok(), true);
        assert_eq!(create_object_result.is_ok(), true);
        assert_eq!(stored_object.title(), "test object title");
        assert_eq!(stored_object.owner_pub_key(), &p);
        assert_eq!(stored_object.points(), [Point::new(1, 2).into(), Point::new(3, 4).into()]);
        assert_eq!(stored_object.deleted(), false);
        assert_eq!(stored_object.history_hash(), &history_hash);
        assert_eq!(stored_owner.ownership_hash(), &ownership_hash);
    }

    #[test]
    fn test_tx_modify_object(){

        // Arrange
        let b = ObjectsBlockchain { db: MemoryDB::new() };
        let v = b.view();
        let (p, s) = gen_keypair();
        let tx_create_owner = TxCreateOwner::new(&p, "test owner name", &s);
        let object_id = 0_u64;
        let wrong_object_id = 1_u64;
        let points = [Point::new(1, 2).into(), Point::new(3, 4).into()];
        let tx_create_object = TxCreateObject::new(&p, "test object title", &points, &p, &s);
        execute_tx::<MemoryDB>(&v, ObjectTx::CreateOwner(tx_create_owner.clone())).unwrap();
        execute_tx::<MemoryDB>(&v, ObjectTx::CreateObject(tx_create_object.clone())).unwrap();
        let modified_title = "modified object title";
        let modified_points = [Point::new(5, 6).into(), Point::new(7, 8).into()];
        let test_tx_modify_object = TxModifyObject::new(&p, object_id, &modified_title, &modified_points, &s);
        let failed_tx_modify_object = TxModifyObject::new(&p, wrong_object_id, &modified_title, &modified_points, &s);

        // Act
        let ok_modification_result = execute_tx::<MemoryDB>(&v, ObjectTx::ModifyObject(test_tx_modify_object.clone()));
        let err_modification_result = execute_tx::<MemoryDB>(&v, ObjectTx::ModifyObject(failed_tx_modify_object.clone()));
        let modified_object = v.objects().get(object_id).unwrap().unwrap();
        let history_hash = v.object_history(object_id).root_hash().unwrap();

        // Assert
        assert_eq!(ok_modification_result.is_ok(), true);
        assert_eq!(err_modification_result.is_ok(), false);
        assert_eq!(modified_object.title(), "modified object title");
        assert_eq!(modified_object.owner_pub_key(), &p);
        assert_eq!(modified_object.points(), [Point::new(5, 6).into(), Point::new(7, 8).into()]);
        assert_eq!(modified_object.deleted(), false);
        assert_eq!(modified_object.history_hash(), &history_hash);
    }

    #[test]
    fn test_tx_transfer_object(){

        // Arrange
        let b = ObjectsBlockchain { db: MemoryDB::new() };
        let v = b.view();
        let (p, s) = gen_keypair();
        let (p2, s2) = gen_keypair();
        let tx_create_owner = TxCreateOwner::new(&p, "test owner name 1", &s);
        let tx_create_owner2 = TxCreateOwner::new(&p2, "test owner name 2", &s2);
        let object_id = 0_u64;
        let wrong_object_id = 1_u64;
        let points = [Point::new(1, 2).into(), Point::new(3, 4).into()];
        let tx_create_object = TxCreateObject::new(&p, "test object title", &points, &p, &s);
        execute_tx::<MemoryDB>(&v, ObjectTx::CreateOwner(tx_create_owner.clone())).unwrap();
        execute_tx::<MemoryDB>(&v, ObjectTx::CreateOwner(tx_create_owner2.clone())).unwrap();
        execute_tx::<MemoryDB>(&v, ObjectTx::CreateObject(tx_create_object.clone())).unwrap();
        let test_tx_transfer_object = TxTransferObject::new(&p, object_id, &p2, &s);
        let failed_tx_transfer_object = TxTransferObject::new(&p, wrong_object_id, &p2, &s);

        // Act
        let ok_transfer_result = execute_tx::<MemoryDB>(&v, ObjectTx::TransferObject(test_tx_transfer_object.clone()));
        let err_transfer_result = execute_tx::<MemoryDB>(&v, ObjectTx::TransferObject(failed_tx_transfer_object.clone()));
        let modified_object = v.objects().get(object_id).unwrap().unwrap();
        let modified_owner = v.owners().get(&p).unwrap().unwrap();
        let modified_owner2 = v.owners().get(&p2).unwrap().unwrap();
        let history_hash = v.object_history(object_id).root_hash().unwrap();
        let ownership_hash = v.owner_objects(&p).root_hash().unwrap();
        let ownership_hash2 = v.owner_objects(&p2).root_hash().unwrap();

        // Assert
        assert_eq!(ok_transfer_result.is_ok(), true);
        assert_eq!(err_transfer_result.is_ok(), false);
        assert_eq!(modified_object.owner_pub_key(), &p2);
        assert_eq!(modified_object.history_hash(), &history_hash);
        assert_eq!(modified_owner.ownership_hash(), &ownership_hash);
        assert_eq!(modified_owner2.ownership_hash(), &ownership_hash2);
    }

    #[test]
    fn test_tx_remove_object(){
        // Arrange
        let b = ObjectsBlockchain { db: MemoryDB::new() };
        let v = b.view();
        let (p, s) = gen_keypair();
        let tx_create_owner = TxCreateOwner::new(&p, "test owner name", &s);
        let object_id = 0_u64;
        let wrong_object_id = 1_u64;
        let points = [Point::new(1, 2).into(), Point::new(3, 4).into()];
        let tx_create_object = TxCreateObject::new(&p, "test object title", &points, &p, &s);
        execute_tx::<MemoryDB>(&v, ObjectTx::CreateOwner(tx_create_owner.clone())).unwrap();
        execute_tx::<MemoryDB>(&v, ObjectTx::CreateObject(tx_create_object.clone())).unwrap();
        let test_tx_remove_object = TxRemoveObject::new(&p, object_id, &s);
        let failed_tx_remove_object = TxRemoveObject::new(&p, wrong_object_id, &s);
        // Act
        let ok_remove_result = execute_tx::<MemoryDB>(&v, ObjectTx::RemoveObject(test_tx_remove_object.clone()));
        let err_remove_result = execute_tx::<MemoryDB>(&v, ObjectTx::RemoveObject(failed_tx_remove_object.clone()));
        let removed_object = v.objects().get(object_id).unwrap().unwrap();
        let history_hash = v.object_history(object_id).root_hash().unwrap();
        // Assert
        assert_eq!(ok_remove_result.is_ok(), true);
        assert_eq!(err_remove_result.is_ok(), false);
        assert_eq!(removed_object.title(), "test object title");
        assert_eq!(removed_object.owner_pub_key(), &p);
        assert_eq!(removed_object.points(), [Point::new(1, 2).into(), Point::new(3, 4).into()]);
        assert_eq!(removed_object.deleted(), true);
        assert_eq!(removed_object.history_hash(), &history_hash);

    }
}









