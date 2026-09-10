use pyo3::{
    prelude::*,
    types::{PyInt, PyList, PyTuple},
};

use super::{
    expression::{PyPipelineExpr, alias_name},
    recipe::{Function, Recipe},
};
use crate::{UnsupportedCapabilityError, errors::ConstructionError};

pub(super) fn integer(value: &Bound<'_, PyAny>) -> PyResult<i64> {
    if !value.is_exact_instance_of::<PyInt>() {
        return Err(ConstructionError::new_err(
            "pipeline integer argument requires an exact int",
        ));
    }
    value
        .extract::<i64>()
        .map_err(|_| ConstructionError::new_err("pipeline integer argument exceeds i64"))
}

#[pyfunction]
pub(super) fn pipeline_literal(value: &Bound<'_, PyAny>) -> PyResult<PyPipelineExpr> {
    if value.is_instance_of::<PyPipelineExpr>() {
        return Err(ConstructionError::new_err(
            "pipeline literal requires a scalar value",
        ));
    }
    PyPipelineExpr::coerce(value)
}

#[pyfunction]
pub(super) fn pipeline_alias(name: &Bound<'_, PyAny>) -> PyResult<PyPipelineExpr> {
    PyPipelineExpr::compose(Recipe::Alias(alias_name(name)?), &[])
}

#[pyfunction]
pub(super) fn pipeline_col(
    table: &Bound<'_, PyAny>,
    column: &Bound<'_, PyAny>,
) -> PyResult<PyPipelineExpr> {
    PyPipelineExpr::compose(Recipe::Column(alias_name(table)?, alias_name(column)?), &[])
}

#[pyfunction]
pub(super) fn pipeline_role(role: &str, name: &Bound<'_, PyAny>) -> PyResult<PyPipelineExpr> {
    let name = alias_name(name)?;
    PyPipelineExpr::compose(
        match role {
            "this" => Recipe::This(name),
            "that" => Recipe::That(name),
            _ => {
                return Err(UnsupportedCapabilityError::new_err(
                    "unsupported pipeline relation role",
                ));
            }
        },
        &[],
    )
}

#[pyfunction]
#[pyo3(signature = (name, *arguments))]
pub(super) fn pipeline_function(
    name: &str,
    arguments: &Bound<'_, PyTuple>,
) -> PyResult<PyPipelineExpr> {
    if matches!(name, "count_rows" | "row_number") {
        if !arguments.is_empty() {
            return Err(ConstructionError::new_err("wrong pipeline function arity"));
        }
        return PyPipelineExpr::compose(
            if name == "count_rows" {
                Recipe::CountRows
            } else {
                Recipe::RowNumber
            },
            &[],
        );
    }
    let (function, arity) = match name {
        "sum" => (Function::Sum, 1),
        "min" => (Function::Min, 1),
        "max" => (Function::Max, 1),
        "average" => (Function::Average, 1),
        "stddev" => (Function::Stddev, 1),
        "count" => (Function::Count, 1),
        "count_distinct" => (Function::CountDistinct, 1),
        "rank" => (Function::Rank, 1),
        "rank_dense" => (Function::RankDense, 1),
        "first" => (Function::First, 1),
        "last" => (Function::Last, 1),
        "lag" | "lead" => {
            if arguments.len() != 2 {
                return Err(ConstructionError::new_err(
                    "lag/lead require an offset and an expression",
                ));
            }
            let offset = integer(&arguments.get_item(0)?)?;
            (
                if name == "lag" {
                    Function::Lag(offset)
                } else {
                    Function::Lead(offset)
                },
                2,
            )
        }
        _ => {
            return Err(UnsupportedCapabilityError::new_err(
                "unsupported pipeline function",
            ));
        }
    };
    if arguments.len() != arity {
        return Err(ConstructionError::new_err("wrong pipeline function arity"));
    }
    let operand = PyPipelineExpr::coerce(&arguments.get_item(arity - 1)?)?;
    PyPipelineExpr::compose(
        Recipe::Function(function, operand.recipe.clone()),
        &[operand],
    )
}

#[pyfunction]
pub(super) fn pipeline_case(
    arms: &Bound<'_, PyAny>,
    otherwise: &Bound<'_, PyAny>,
) -> PyResult<PyPipelineExpr> {
    if !arms.is_instance_of::<PyList>() && !arms.is_instance_of::<PyTuple>() {
        return Err(ConstructionError::new_err(
            "pipeline CASE arms require a list or tuple",
        ));
    }
    let fallback = PyPipelineExpr::coerce(otherwise)?;
    let mut operands = vec![fallback.clone()];
    let mut pairs = Vec::new();
    for arm in arms.try_iter()? {
        let arm = arm?;
        if (!arm.is_instance_of::<PyList>() && !arm.is_instance_of::<PyTuple>()) || arm.len()? != 2
        {
            return Err(ConstructionError::new_err(
                "each CASE arm requires condition and value",
            ));
        }
        let condition = PyPipelineExpr::coerce(&arm.get_item(0)?)?;
        let value = PyPipelineExpr::coerce(&arm.get_item(1)?)?;
        pairs.push((condition.recipe.clone(), value.recipe.clone()));
        operands.extend([condition, value]);
    }
    PyPipelineExpr::compose(Recipe::Case(pairs, fallback.recipe), &operands)
}
