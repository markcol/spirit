//! Helpers for integrating common configuration patterns.
//!
//! There are some common patterns of integrating pieces of configuration into an application and
//! make them active. Many of these patterns require registering in multiple callbacks at once to
//! work correctly. Doing it manually is tedious and error prone.
//!
//! The traits in this module allow registering all the callbacks in one go, making it easier for
//! other crates to integrate such patterns.
use std::any::TypeId;
use std::borrow::Borrow;
use std::fmt::{Debug, Display};

use arc_swap::ArcSwap;
use serde::Deserialize;
use structopt::StructOpt;

use super::Builder;

/// The basic helper trait.
///
/// It allows being plugged into a builder and modifying it in an arbitrary way.
///
/// It is more common to apply the helper by the
/// [`Builder::with`](../struct.Builder.html#method.with) method than directly.
///
/// There's an implementation of `Helper` for `FnOnce(Builder) -> Builder`, so helpers can be
/// either custom types or just closures (which are often more convenient than defining an empty
/// type and the implementation).
///
/// ```rust
/// use std::borrow::Borrow;
///
/// use spirit::{ArcSwap, Builder, Empty, Spirit};
/// use spirit::helpers::Helper;
///
/// struct CfgPrint;
///
/// impl<S> Helper<S, Empty, Empty> for CfgPrint
/// where
///     S: Borrow<ArcSwap<Empty>> + Sync + Send + 'static,
/// {
///     fn apply(self, builder: Builder<S, Empty, Empty>) -> Builder<S, Empty, Empty> {
///         builder.on_config(|_config| println!("Config changed"))
///     }
/// }
///
/// Spirit::<_, Empty, _>::new(Empty {})
///     .with(CfgPrint)
///     .run(|_spirit| {
///         println!("Running...");
///         Ok(())
///     })
/// ```
///
/// ```rust
/// use std::borrow::Borrow;
///
/// use spirit::{ArcSwap, Builder, Empty, Spirit};
///
/// fn cfg_print<S>(builder: Builder<S, Empty, Empty>) -> Builder<S, Empty, Empty>
/// where
///     S: Borrow<ArcSwap<Empty>> + Sync + Send + 'static,
/// {
///     builder.on_config(|_config| println!("Config changed"))
/// }
///
/// Spirit::<_, Empty, _>::new(Empty {})
///     .with(cfg_print)
///     .run(|_spirit| {
///         println!("Running...");
///         Ok(())
///     })
/// ```
pub trait Helper<S, O, C>
where
    S: Borrow<ArcSwap<C>> + Sync + Send + 'static,
{
    /// Perform the transformation on the given builder.
    ///
    /// And yes, it is possible to do multiple primitive transformations inside one helper (this is
    /// what makes helpers useful for 3rd party crates, they can integrate with just one call of
    /// [`with`](../struct.Builder.html#method.with)).
    fn apply(self, builder: Builder<S, O, C>) -> Builder<S, O, C>;
}

impl<S, O, C, F> Helper<S, O, C> for F
where
    S: Borrow<ArcSwap<C>> + Sync + Send + 'static,
    F: FnOnce(Builder<S, O, C>) -> Builder<S, O, C>,
{
    fn apply(self, builder: Builder<S, O, C>) -> Builder<S, O, C> {
        self(builder)
    }
}

/// A specialized version of [`Helper`](trait.Helper.html) for a piece of extracted configuration.
///
/// This traits works in tandem with an extractor function and action. The extractor is supposed to
/// extract a specific piece of configuration. The trait is defined on the type returned by the
/// extractor and produces some kind of resource. The action is then performed with the resource.
///
/// As an example, the type implementing the trait could be a configuration for a TCP socket. The
/// extractor just pulls out the instance of the type out of the configuration. The action could be
/// whatever the application needs to do with the TCP socket. The helper then bridges these
/// together by making the socket out of the configuration.
///
/// The trait often delegates to the basic version of `Helper` under the hood, by connecting the
/// extractor with the „active“ part of the helper.
///
/// You can use the [`Builder::config_helper`](../struct.Builder.html#method.config_helper) to
/// apply a `CfgHelper`.
///
/// # TODO
///
/// This calls for an example.
///
/// # Future plans
///
/// It is planned to eventually have a custom derive for these kinds of helpers to compose a helper
/// of a bigger piece of configuration. The extractor would then be auto-generated.
pub trait CfgHelper<S, O, C, Action>
where
    S: Borrow<ArcSwap<C>> + Sync + Send + 'static,
{
    /// Perform the creation and application of the helper.
    ///
    /// # Params
    ///
    /// * `extractor`: Function that pulls out a bit of configuration out of the complete
    ///   configuration type.
    /// * `action`: Something application-specific performed with the resource built of the
    ///   relevant piece of configuration.
    /// * `name`: Named used in logs to reference the specific instance of the type in logs. It is
    ///   more useful to have „heartbeat connection“ instead of „tcp socket“ in there (often,
    ///   application has many different kinds of tcp sockets around).
    /// * `builder`: The builder to modify by this helper.
    fn apply<Extractor, Name>(
        extractor: Extractor,
        action: Action,
        name: Name,
        builder: Builder<S, O, C>,
    ) -> Builder<S, O, C>
    where
        Extractor: FnMut(&C) -> Self + Send + 'static,
        Name: Clone + Display + Send + Sync + 'static;
}

/// A variant of the [`CfgHelper`](trait.CfgHelper.html) for resources that come in groups.
///
/// If an application should (for example) listen for incoming connections, it is often desirable
/// to be able to configure multiple listening endpoints at once.
///
/// In simple words, if the `IteratedCfgHelper` is implemented for a type, a `CfgHelper` is
/// implemented for a container of the type (eg. `Vec`). The extractor then extracts the vector and
/// the helper takes care of managing multiple instances of the resource.
///
/// # Single instance
///
/// If a helper is implemented in terms of `IteratedCfgHelper` and your application configuration
/// contains exactly one instance, it is possible to return `iter::once` from the extractor, which
/// will pretend the configuration contains a container of exactly one thing.
///
/// Some helper crates may already provide both implementations on the same type in this manner.
pub trait IteratedCfgHelper<S, O, C, Action>
where
    S: Borrow<ArcSwap<C>> + Sync + Send + 'static,
{
    /// Perform the transformation of the builder.
    ///
    /// It works the same way as [`CfgHelper::apply`](trait.CfgHelper.html#method.apply), only with
    /// slightly different types around the extractor.
    fn apply<Extractor, ExtractedIter, Name>(
        extractor: Extractor,
        action: Action,
        name: Name,
        builder: Builder<S, O, C>,
    ) -> Builder<S, O, C>
    where
        Self: Sized, // TODO: Why does rustc insist on this one?
        Extractor: FnMut(&C) -> ExtractedIter + Send + 'static,
        ExtractedIter: IntoIterator<Item = Self>,
        Name: Clone + Display + Send + Sync + 'static;
}

impl<S, O, C, Action, Iter, Target> CfgHelper<S, O, C, Action> for Iter
where
    S: Borrow<ArcSwap<C>> + Sync + Send + 'static,
    Iter: IntoIterator<Item = Target>,
    Target: IteratedCfgHelper<S, O, C, Action>,
{
    fn apply<Extractor, Name>(
        extractor: Extractor,
        action: Action,
        name: Name,
        builder: Builder<S, O, C>,
    ) -> Builder<S, O, C>
    where
        Extractor: FnMut(&C) -> Self + Send + 'static,
        Name: Clone + Display + Send + Sync + 'static,
    {
        <Target as IteratedCfgHelper<S, O, C, Action>>::apply(extractor, action, name, builder)
    }
}

impl<S, O, C> Builder<S, O, C>
where
    S: Borrow<ArcSwap<C>> + Sync + Send + 'static,
    for<'de> C: Deserialize<'de> + Send + Sync + 'static,
    O: Debug + StructOpt + Sync + Send + 'static,
{
    /// Apply a config helper to the builder.
    ///
    /// For more information see [`CfgHelper`](helpers/trait.CfgHelper.html).
    pub fn config_helper<Cfg, Extractor, Action, Name>(
        self,
        extractor: Extractor,
        action: Action,
        name: Name,
    ) -> Self
    where
        Extractor: FnMut(&C) -> Cfg + Send + 'static,
        Cfg: CfgHelper<S, O, C, Action>,
        Name: Clone + Display + Send + Sync + 'static,
    {
        trace!("Adding config helper for {}", name);
        CfgHelper::apply(extractor, action, name, self)
    }

    /// Check if this is the first call with the given type.
    ///
    /// Some helpers share common part. This common part makes sense to register just once, so this
    /// can be used to check that. The first call with given type returns `true`, any future ones
    /// with the same type return `false`.
    ///
    /// The method has no direct effect on the future spirit constructed from the builder and
    /// works only as a note for future helpers that want to manipulate the builder.
    ///
    /// A higher-level interface is the [`with_singleton`](#method.with_singleton) method.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use spirit::{Empty, Spirit};
    ///
    /// let mut builder = Spirit::<_, Empty, _>::new(Empty {});
    ///
    /// struct X;
    /// struct Y;
    ///
    /// assert!(builder.singleton::<X>());
    /// assert!(!builder.singleton::<X>());
    /// assert!(builder.singleton::<Y>());
    /// ```
    pub fn singleton<T: 'static>(&mut self) -> bool {
        self.singletons.insert(TypeId::of::<T>())
    }

    /// Apply a ['Helper`](helpers/trait.Helper.html) to the builder.
    pub fn with<H: Helper<S, O, C>>(self, helper: H) -> Self {
        trace!("Adding a helper");
        helper.apply(self)
    }

    /// Apply the first [`Helper`](helpers.trait.Helper.html) of the type.
    ///
    /// This applies the passed helper, but only if a helper with the same hasn't yet been applied
    /// (or the [`singleton`](#method.singleton) called manually).
    ///
    /// Note that different instances of the same type of a helper can act differently, but are
    /// still considered the same type. This means the first instance wins. This is considered a
    /// feature ‒ many other helpers need some environment to run in (like `tokio`). The helpers
    /// try to apply a default configuration, but the user can apply a specific configuration
    /// first.
    pub fn with_singleton<T: Helper<S, O, C> + 'static>(mut self, singleton: T) -> Self {
        if self.singleton::<T>() {
            self.with(singleton)
        } else {
            trace!("Singleton already exists");
            self
        }
    }
}
