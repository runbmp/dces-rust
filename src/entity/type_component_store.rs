use core::{any::{Any, TypeId}, cell::RefCell};

use std::collections::HashMap;

use super::{Component, Entity, EntityStore, SharedComponentBox, ComponentBox};
use crate::error::NotFound;

/// The type key based entity builder is used to create an entity with components.
pub struct TypeEntityBuilder<'a, T>
where
    T: EntityStore,
{
    /// The created entity.
    pub entity: Entity,

    /// Reference to the component store.
    pub component_store: &'a mut TypeComponentStore,

    /// Reference to the entity store.
    pub entity_store: &'a mut T,
}

impl<'a, T> TypeEntityBuilder<'a, T>
where
    T: EntityStore,
{
    /// Adds a component of type `C` to the entity.
    pub fn with<C: Component>(self, component: C) -> Self {
        self.component_store
            .register_component(self.entity, component);
        self
    }

    /// Adds an entity as `source` for a shared component of type `C`.
    pub fn with_shared<C: Component>(self, source: Entity) -> Self {
        self.component_store
            .register_shared_component::<C>(self.entity, source);
        self
    }

    /// Adds an entity as `source` for a shared component box.
    pub fn with_shared_box(self, source: SharedComponentBox) -> Self {
        self.component_store
            .register_shared_component_box(self.entity, source);
        self
    }

    /// Adds a component box to the entity.
    pub fn with_box(self, component_box: ComponentBox) -> Self {
        self.component_store
            .register_component_box(self.entity, component_box);
        self
    }

    /// Finishing the creation of the entity.
    pub fn build(self) -> Entity {
        self.entity_store
            .register_entity(self.entity);
        self.entity
    }
}

/// The `TypeComponentStore` stores the components of all entities. It could be used to
/// borrow the components of the entities.
#[derive(Default, Debug)]
pub struct TypeComponentStore {
    components: HashMap<Entity, HashMap<TypeId, Box<dyn Any>>>,
    shared: HashMap<Entity, RefCell<HashMap<TypeId, Entity>>>,
}

impl TypeComponentStore {
    /// Registers an new entity on the store.
    pub fn register_entity(&mut self, entity: impl Into<Entity>) {
        self.components.insert(entity.into(), HashMap::new());
    }

    /// Removes and entity from the store.
    pub fn remove_entity(&mut self, entity: impl Into<Entity>) {
        self.components.remove(&entity.into());
    }

    /// Register a `component` for the given `entity`.
    pub fn register_component<C: Component>(&mut self, entity: Entity, component: C) {
        self.components
            .get_mut(&entity)
            .get_or_insert(&mut HashMap::new())
            .insert(TypeId::of::<C>(), Box::new(component));
    }

    /// Registers a sharing of the given component between the given entities.
    pub fn register_shared_component<C: Component>(&mut self, target: Entity, source: Entity) {
        if !self.shared.contains_key(&target) {
            self.shared.insert(target, RefCell::new(HashMap::new()));
        }

        // Removes unshared component of entity.
        if let Some(comp) = self.components.get_mut(&target) {
            comp.remove(&TypeId::of::<C>());
        }

        self.shared[&target]
            .borrow_mut()
            .insert(TypeId::of::<C>(), source);
    }

    /// Registers a sharing of the given component between the given entities.
    pub fn register_shared_component_box(
        &mut self,
        target: impl Into<Entity>,
        source: SharedComponentBox,
    ) {
        let target = target.into();
        if !self.shared.contains_key(&target) {
            self.shared.insert(target, RefCell::new(HashMap::new()));
        }

        // Removes unshared component of entity.
        if let Some(comp) = self.components.get_mut(&target) {
            comp.remove(&source.type_id);
        }

        self.shared[&target]
            .borrow_mut()
            .insert(source.type_id, source.source);
    }

    /// Register a `component_box` for the given `entity`.
    pub fn register_component_box(
        &mut self,
        entity: impl Into<Entity>,
        component_box: ComponentBox,
    ) {
        let entity = entity.into();
        let (type_id, component) = component_box.consume();

        self.components
            .get_mut(&entity)
            .get_or_insert(&mut HashMap::new())
            .insert(type_id, component);
    }

    /// Returns the number of components in the store.
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Returns `true` if the store contains the specific entity.
    pub fn contains_entity(&self, entity: &Entity) -> bool {
        self.components.contains_key(entity)
    }

    /// Returns `true` if entity is the origin of the requested component `false`.
    pub fn is_origin<C: Component>(&self, entity: Entity) -> bool {
        if let Some(components) = self.components.get(&entity) {
            return components.contains_key(&TypeId::of::<C>());
        }

        false
    }

    // Search the the target entity in the entity map.
    fn target_entity_from_shared<C: Component>(&self, entity: Entity) -> Result<Entity, NotFound> {
        self.shared
            .get(&entity)
            .ok_or_else(|| NotFound::Entity(entity))
            .and_then(|en| {
                en.borrow()
                    .get(&TypeId::of::<C>())
                    .map(|entity| *entity)
                    .ok_or_else(|| NotFound::Component(TypeId::of::<C>()))
            })
    }

    // Returns the target entity. First search in entities map. If not found search in shared entity map.
    fn target_entity<C: Component>(&self, entity: Entity) -> Result<Entity, NotFound> {
        if !self.components.contains_key(&entity)
            || !self.components[&entity].contains_key(&TypeId::of::<C>())
        {
            return self.target_entity_from_shared::<C>(entity);
        }

        Result::Ok(entity)
    }

    /// Returns a reference of a component of type `C` from the given `entity`. If the entity does
    /// not exists or it doesn't have a component of type `C` `NotFound` will be returned.
    pub fn borrow_component<C: Component>(&self, entity: Entity) -> Result<&C, NotFound> {
        let target_entity = self.target_entity::<C>(entity);

        match target_entity {
            Ok(entity) => self
                .components
                .get(&entity)
                .ok_or_else(|| NotFound::Entity(entity))
                .and_then(|en| {
                    en.get(&TypeId::of::<C>())
                        .map(|component| {
                            component.downcast_ref().expect(
                                "EntityComponentManager.borrow_component: internal downcast error",
                            )
                        })
                        .ok_or_else(|| NotFound::Component(TypeId::of::<C>()))
                }),
            Err(_) => Result::Err(NotFound::Entity(entity)),
        }
    }

    /// Returns a mutable reference of a component of type `C` from the given `entity`. If the entity does
    /// not exists or it doesn't have a component of type `C` `NotFound` will be returned.
    pub fn borrow_mut_component<C: Component>(
        &mut self,
        entity: Entity,
    ) -> Result<&mut C, NotFound> {
        let target_entity = self.target_entity::<C>(entity);

        match target_entity {
            Ok(entity) => self
                .components
                .get_mut(&entity)
                .ok_or_else(|| NotFound::Entity(entity))
                .and_then(|en| {
                    en.get_mut(&TypeId::of::<C>())
                        .map(|component| {
                            component.downcast_mut().expect(
                            "EntityComponentManager.borrow_mut_component: internal downcast error",
                        )
                        })
                        .ok_or_else(|| NotFound::Component(TypeId::of::<C>()))
                }),
            Err(_) => Result::Err(NotFound::Entity(entity)),
        }
    }
}
