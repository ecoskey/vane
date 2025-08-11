use std::ops::{Bound, RangeBounds};

use bevy_ecs::{
    component::Component, error::BevyError, lifecycle::Insert, observer::On, system::Query,
};
use bevy_math::{Vec3, VectorSpace};

use crate::{
    flow::FlowVector,
    vane::{Vane, VaneSample, VaneUpdate},
};

pub trait Measure: 'static {
    type Value: VectorSpace<Scalar: Send + Sync> + Send + Sync;

    fn measure<'a>(
        vane: &'a Vane,
        samples: impl ExactSizeIterator<Item = &'a VaneSample>,
    ) -> Self::Value;
}

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum MeasureError {
    #[error("This measure does not support the provided Vane type: {0:?}")]
    UnsupportedVane(Vane),
}

fn update_measure_state<M: Measure>(
    ev: On<Insert, (Vane, Measured<M>)>,
    mut vanes: Query<(&Vane, &mut Measured<M>)>,
) -> Result<(), BevyError> {
    // let (_vane, status, mut measured) = vanes.get_mut(ev.target())?;
    // measured.active = matches!(status, VaneStatus::Enabled); // && M::supports_vane(vane);
    // Ok(())
    todo!();
}

pub type Scalar<V> = <V as VectorSpace>::Scalar;

#[derive(Component)]
pub struct Measured<M: Measure> {
    value: Option<M::Value>,
}

impl<M: Measure> Default for Measured<M> {
    fn default() -> Self {
        Self { value: None }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("value has not been measured yet")]
pub struct MissingMeasuredValueError;

impl<M: Measure> Measured<M> {
    pub fn value(&self) -> Result<M::Value, MissingMeasuredValueError> {
        self.value.ok_or(MissingMeasuredValueError)
    }
}

fn update_measure<M: Measure>(ev: On<VaneUpdate>) {}

#[derive(Component)]
#[component(immutable)]
#[require(TriggerData<M>)]
pub struct Trigger<M: Measure> {
    pub start: Bound<Scalar<M::Value>>,
    pub end: Bound<Scalar<M::Value>>,
}

impl<M: Measure> Trigger<M> {
    pub fn from_bounds(range: impl RangeBounds<Scalar<M::Value>>) -> Self {
        Self {
            start: range.start_bound().cloned(),
            end: range.start_bound().cloned(),
        }
    }
}

impl<M: Measure> RangeBounds<Scalar<M::Value>> for Trigger<M> {
    #[inline]
    fn start_bound(&self) -> Bound<&Scalar<M::Value>> {
        self.start.as_ref()
    }

    #[inline]
    fn end_bound(&self) -> Bound<&Scalar<M::Value>> {
        self.end.as_ref()
    }
}

#[derive(Component)]
struct TriggerData<M: Measure> {
    value: Option<Scalar<M::Value>>,
    in_range: bool,
}

impl<M: Measure> Default for TriggerData<M> {
    fn default() -> Self {
        Self {
            value: None,
            in_range: false,
        }
    }
}

// Measure impls

impl Measure for FlowVector {
    type Value = Self;

    #[inline]
    fn measure<'a>(
        _vane: &'a Vane,
        samples: impl ExactSizeIterator<Item = &'a VaneSample>,
    ) -> Self::Value {
        let n_samples = samples.len() as f32;
        samples.map(|sample| sample.flow).sum::<FlowVector>() / n_samples
    }
}

pub struct MomentumDensity;

impl Measure for MomentumDensity {
    type Value = Vec3;

    #[inline]
    fn measure<'a>(
        vane: &'a Vane,
        samples: impl ExactSizeIterator<Item = &'a VaneSample>,
    ) -> Self::Value {
        FlowVector::measure(vane, samples).momentum_density()
    }
}

pub struct Density;

impl Measure for Density {
    type Value = f32;

    #[inline]
    fn measure<'a>(
        vane: &'a Vane,
        samples: impl ExactSizeIterator<Item = &'a VaneSample>,
    ) -> Self::Value {
        FlowVector::measure(vane, samples).density()
    }
}

pub struct Velocity;

impl Measure for Velocity {
    type Value = Vec3;

    #[inline]
    fn measure<'a>(
        vane: &'a Vane,
        samples: impl ExactSizeIterator<Item = &'a VaneSample>,
    ) -> Self::Value {
        FlowVector::measure(vane, samples).velocity()
    }
}
