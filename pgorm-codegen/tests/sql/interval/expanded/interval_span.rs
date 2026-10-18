use pgorm::entity::prelude::*;

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "interval_span"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Eq)]
pub struct Model {
    pub id: i32,
    pub plain: Interval,
    pub maybe: Option<Interval>,
    pub precise: Interval,
    pub months: Interval,
    pub days: Interval,
    pub clock: Option<Interval>,
    pub minutes: Interval,
    pub seconds: Interval,
    pub history: Vec<Interval>,
    pub budgets: Option<Vec<Interval>>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    Plain,
    Maybe,
    Precise,
    Months,
    Days,
    Clock,
    Minutes,
    Seconds,
    History,
    Budgets,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Id,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = i32;

    fn auto_increment() -> bool {
        false
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Id => ColumnType::Integer.def(),
            Self::Plain => ColumnType::Interval(IntervalSpec::Any(None)).def(),
            Self::Maybe => ColumnType::Interval(IntervalSpec::Any(None)).def().null(),
            Self::Precise => ColumnType::Interval(IntervalSpec::Any(Some(IntervalPrecision::P3))).def(),
            Self::Months => ColumnType::Interval(IntervalSpec::Fields(PgInterval::YearToMonth)).def(),
            Self::Days => ColumnType::Interval(IntervalSpec::Fields(PgInterval::Day)).def(),
            Self::Clock => ColumnType::Interval(IntervalSpec::Fields(PgInterval::DayToSecond(Some(IntervalPrecision::P3))))
                .def()
                .null(),
            Self::Minutes => ColumnType::Interval(IntervalSpec::Fields(PgInterval::HourToMinute)).def(),
            Self::Seconds => ColumnType::Interval(IntervalSpec::Fields(PgInterval::Second(Some(IntervalPrecision::P0)))).def(),
            Self::History => ColumnType::Array(Arc::new(ColumnType::Interval(IntervalSpec::Any(None)))).def(),
            Self::Budgets => ColumnType::Array(Arc::new(ColumnType::Interval(IntervalSpec::Fields(PgInterval::YearToMonth))))
                .def()
                .null(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No RelationDef")
    }
}

impl ActiveModelBehavior for ActiveModel {}
