"""Programs built through native Python APIs, compared with independent Rust."""


def programs(n):
    table = n.Table("items")
    base = n.Pipeline(table)
    col = n.pipeline_col
    alias = n.pipeline_alias
    call = n.pipeline_function
    key, category, amount = (
        col("items", name) for name in ("id", "category", "amount")
    )
    total, rank = alias("total"), alias("row_rank")

    def repeated(b):
        b.bind("unused")
        value = b.bind(n.Value(2, "i32"))
        return key.gt(value) & key.lt(value + 10)

    left = base.filter_with(lambda b: key.gt(b.bind(n.Value(1, "i32")))).select(
        key, category, amount
    )
    right = base.filter_with(lambda b: key.lt(b.bind(n.Value(8, "i32")))).select(
        key, category, amount
    )
    return {
        "literal": base.filter(key > 2).select(key, amount).sort(key.desc()).take(3),
        "repeated": base.filter_with(repeated),
        "literal_string": base.filter(category == "O'Brien; -- 雪"),
        "derive": base.derive((amount + 2).as_(total)).filter(total > 5),
        "derive_with": base.derive_with(lambda b: [(amount + b.bind(2)).as_(total)]),
        "select_with": base.select_with(
            lambda b: [key, b.bind("payload").as_("payload")]
        ),
        "group": base.group(category)
        .aggregate(call("sum", amount).as_(total))
        .filter(total > 3),
        "group_with": base.group_with(
            lambda b: [category.coalesce(b.bind("missing"))]
        ).aggregate_with(lambda b: [(call("sum", amount) + b.bind(1)).as_(total)]),
        "window": base.window(
            call("row_number").as_(rank),
            over=n.PipelineOver().by(category).sort_by(key).rows(None, 0),
        ).filter(rank <= 2),
        "window_with": base.window_with(
            n.PipelineOver().sort_by(key),
            lambda b: [(call("sum", amount) + b.bind(2)).as_(total)],
        ),
        "sort_with": base.sort_with(lambda b: [key + b.bind(1)]).take_range(2, 4),
        "join": base.join(
            n.pipeline_source(table).named("peer"),
            key == col("peer", "id"),
            kind=n.Join.Left,
        ).select(key, col("peer", "amount").as_("peer_amount")),
        "join_with": base.join_with(
            n.pipeline_source(right).named("peer"),
            lambda b: (key == col("peer", "id")) & (amount > b.bind(1)),
        ),
        "append": left.append(right),
        "intersect": left.intersect(right),
        "remove": left.remove(right),
        "distinct": base.select(category).distinct(),
        "case": base.select(n.pipeline_case([(key > 1, "yes")], "no").as_("answer")),
        "in_array": base.filter_with(
            lambda b: key.in_array(
                [b.bind(n.Value(1, "i32")), b.bind(n.Value(2, "i32"))]
            )
        ),
        "cast_unary": base.select(
            (-amount).cast("bigint").as_("negative"), (~key.is_null()).as_("present")
        ),
        "qualified": n.Pipeline(n.Table('items "β"', schema='schema "β"')).select(
            col('items "β"', 'id "β"').as_('out "β"')
        ),
    }
