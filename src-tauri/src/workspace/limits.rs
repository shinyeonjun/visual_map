use super::model::{CodeInventory, DbInventory};

const INVENTORY_ITEM_LIMIT: usize = 100;

pub(crate) fn bounded_code_inventory(mut inventory: CodeInventory) -> CodeInventory {
    let mut partial = truncate_to(&mut inventory.routes);
    partial |= truncate_to(&mut inventory.services);
    partial |= truncate_to(&mut inventory.files);
    partial |= truncate_to(&mut inventory.handlers);
    partial |= truncate_to(&mut inventory.repositories);
    partial |= truncate_to(&mut inventory.functions);
    partial |= truncate_to(&mut inventory.classes);
    partial |= truncate_to(&mut inventory.modules);
    partial |= truncate_to(&mut inventory.unknown);

    let retained = inventory
        .routes
        .iter()
        .chain(inventory.services.iter())
        .chain(inventory.files.iter())
        .chain(inventory.handlers.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .map(|item| item.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    inventory.calls.retain(|call| {
        retained.contains(call.from.as_str()) && retained.contains(call.to.as_str())
    });
    inventory.handles.retain(|handle| {
        retained.contains(handle.route.as_str()) && retained.contains(handle.handler.as_str())
    });
    inventory.partial = partial;
    inventory
}

pub(crate) fn bounded_db_inventory(mut inventory: DbInventory) -> DbInventory {
    inventory.tables.truncate(INVENTORY_ITEM_LIMIT);
    inventory
}

fn truncate_to<T>(items: &mut Vec<T>) -> bool {
    let truncated = items.len() > INVENTORY_ITEM_LIMIT;
    items.truncate(INVENTORY_ITEM_LIMIT);
    truncated
}
