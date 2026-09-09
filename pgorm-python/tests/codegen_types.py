"""Type-check this consumer against the installed generated application stubs."""

from typing import assert_type
from pgorm import Connection
from pgorm.app import (
    Account,
    AccountActive,
    AccountModel,
    AccountNotes,
    AccountOnly,
    NoteModel,
    RequiredNotes,
)


async def consumer(connection: Connection) -> None:
    active = Account.active().set_id(1).set_display_name("Nora").set_note(None)
    assert_type(active, AccountActive)
    model = await active.insert(connection)
    assert_type(model, AccountModel)
    assert_type(model.id, int)
    assert_type(model.display_name, str)
    assert_type(model.note, str | None)
    assert_type(model.mood, str)
    assert_type(model.into_active(), AccountActive)
    assert_type(model.with_value("note", "local"), AccountModel)
    assert_type(await Account.find().all(connection), list[AccountModel])
    assert_type(await Account.find().one_opt(connection), AccountModel | None)
    assert_type(
        await AccountNotes.find().all(connection),
        list[tuple[AccountModel, NoteModel | None]],
    )
    assert_type(
        await AccountNotes.find().cursor("id").first(2).all(connection),
        list[tuple[AccountModel, NoteModel | None]],
    )
    assert_type(
        await RequiredNotes.find().all(connection), list[tuple[AccountModel, NoteModel]]
    )
    assert_type(await AccountOnly.find().one_opt(connection), AccountModel | None)
