use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveCreatedRange)]
#[pgorm(range_name = "slot", schema_name = "booking")]
pub struct Slot(pub Range<i32>);

#[derive(Clone, Debug, PartialEq, DeriveCreatedRange)]
#[pgorm(range_name = "floatrange")]
pub struct Floatrange(pub Range<f64>);

#[derive(Clone, Debug, PartialEq, Eq, DeriveCreatedRange)]
#[pgorm(range_name = "textrange")]
pub struct Textrange(pub Range<String>);
