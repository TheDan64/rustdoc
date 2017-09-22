//! Code used to serialize crate data to JSON.

mod api;

pub use self::api::*;

use std::collections::HashMap;
use std::sync::mpsc;

use analysis::{AnalysisHost, DefKind};
use serde_json;

use error::*;

/// This creates the JSON documentation from the given `AnalysisHost`.
pub fn create_json(host: &AnalysisHost, crate_name: &str) -> Result<String> {
    // This function does a lot, so here's the plan:
    //
    // First, we need to process the root def and get its list of children.
    // Then, we process all of the children. Children may produce more children
    // to be processed too. Once we've processed them all, we're done.

    // Step one: we need to get all of the "def roots", and then find the
    // one that's our crate.
    let roots = host.def_roots()?;

    let id = roots.iter().find(|&&(_, ref name)| name == crate_name);
    let root_id = match id {
        Some(&(id, _)) => id,
        _ => return Err(ErrorKind::CrateErr(crate_name.to_string()).into()),
    };

    let root_def = host.get_def(root_id)?;

    // Now that we have that, it's time to get the children; these are
    // the top-level items for the crate.
    let ids = host.for_each_child_def(root_id, |id, _def| {
        id
    }).unwrap();

    // Now, we push all of those children onto a channel. The channel functions
    // as a work queue; we take an item off, process it, and then if it has
    // children, push them onto the queue. When the queue is empty, we've processed
    // everything.

    let (sender, receiver) = mpsc::channel();

    for id in ids {
        sender.send(id).unwrap();
    }

    // the loop below is basically creating this vector
    let mut included: Vec<Document> = Vec::new();
    
    // this is probably the wrong spot for this
    let mut relationships: HashMap<String, Vec<Data>> = HashMap::with_capacity(METADATA_SIZE);
    
    while let Ok(id) = receiver.try_recv() {
        // push each child to be processed itself
        host.for_each_child_def(id, |id, _def| {
            sender.send(id).unwrap();
        })?;

        // process this one

        // question: we could do this by cloning it in the call to for_each_child_def
        // above/below; is that cheaper, or is this cheaper?
        let def = host.get_def(id).unwrap();

        let (ty, relations_key) = match def.kind {
            DefKind::Mod => (String::from("module"), String::from("modules")),
            DefKind::Struct => (String::from("struct"), String::from("structs")),
            _ => continue,
        };

        // Using the item's metadata we create a new `Document` type to be put in the eventual
        // serialized JSON.
        included.push(
            Document::new()
                .ty(ty.clone())
                .id(def.qualname.clone())
                .attributes(String::from("name"), def.name)
                .attributes(String::from("docs"), def.docs),
        );

        let item_relationships = relationships.entry(relations_key).or_insert_with(
            Default::default,
        );
        item_relationships.push(Data::new().ty(ty).id(def.qualname));
    }

    let mut data_document = Document::new()
        .ty(String::from("crate"))
        .id(crate_name.to_string())
        .attributes(String::from("docs"), root_def.docs);

    // Insert all of the different types of relationships into this `Document` type only
    for (ty, data) in relationships {
        data_document.relationships(ty, data);
    }

    Ok(serde_json::to_string(
        &Documentation::new().data(data_document).included(
            included,
        ),
    )?)
}
