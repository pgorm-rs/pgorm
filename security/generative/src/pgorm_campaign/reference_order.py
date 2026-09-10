"""Keep relational sort keys across projections without exposing extra fields."""

from dataclasses import dataclass

from .comparison import InvalidOracle
from .reference_sql import SQL, join
from .reference_values import qualified, quote


@dataclass(frozen=True)
class Order:
    direction: str
    visible: int | None


def retained(ordering, previous, projections, distinct):
    result = []
    for key in ordering:
        value = previous[key.visible] if key.visible is not None else None
        visible = next(
            (index for index, item in enumerate(projections) if item == value), None
        )
        if distinct and visible is None:
            raise InvalidOracle(
                "distinct after projecting away a sort key needs an explicit order oracle"
            )
        result.append(Order(key.direction, visible))
    return tuple(result)


def projections(ordering, qualifier="q"):
    return [
        SQL((qualified("o" + str(index), qualifier),))
        + " AS "
        + quote("o" + str(index))
        for index in range(len(ordering))
    ]


def clause(ordering, qualifier="q"):
    if not ordering:
        return SQL()
    return " ORDER BY " + join(
        [
            SQL((qualified("o" + str(index), qualifier), " " + key.direction))
            for index, key in enumerate(ordering)
        ]
    )


def keys(expressions, context, previous):
    values, ordering = [], []
    for expression in expressions:
        direction = "ASC"
        if expression.operation == "unary" and expression.data["operator"] in (
            "asc",
            "desc",
        ):
            direction = expression.data["operator"].upper()
            expression = expression.inputs["value"]
        value = expression.sql(context)
        visible = next(
            (index for index, item in enumerate(previous) if item == value), None
        )
        values.append(value + " AS " + quote("o" + str(len(ordering))))
        ordering.append(Order(direction, visible))
    return values, tuple(ordering)
