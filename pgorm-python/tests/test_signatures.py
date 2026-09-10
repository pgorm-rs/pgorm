"""Compare installed native call signatures with the stubs shipped beside them."""

import ast
import inspect
import importlib
from pathlib import Path
import unittest

import pgorm
from pgorm import _native


def stub_parameters(function, *, receiver):
    arguments = function.args
    positional = arguments.posonlyargs + arguments.args
    default_start = len(positional) - len(arguments.defaults)
    parameters = []
    for index, argument in enumerate(positional):
        if receiver and index == 0:
            continue
        kind = (
            inspect.Parameter.POSITIONAL_ONLY
            if index < len(arguments.posonlyargs)
            else inspect.Parameter.POSITIONAL_OR_KEYWORD
        )
        parameters.append((argument.arg, kind, index >= default_start))
    if arguments.vararg:
        parameters.append(
            (arguments.vararg.arg, inspect.Parameter.VAR_POSITIONAL, False)
        )
    parameters.extend(
        (argument.arg, inspect.Parameter.KEYWORD_ONLY, default is not None)
        for argument, default in zip(arguments.kwonlyargs, arguments.kw_defaults)
    )
    if arguments.kwarg:
        parameters.append((arguments.kwarg.arg, inspect.Parameter.VAR_KEYWORD, False))
    return parameters


def runtime_parameters(value, *, receiver):
    parameters = list(inspect.signature(value).parameters.values())
    if receiver:
        parameters = parameters[1:]
    return [
        (
            parameter.name,
            parameter.kind,
            parameter.default is not inspect.Parameter.empty,
        )
        for parameter in parameters
    ]


def native_calls(root):
    for path in sorted(root.glob("_*.pyi")):
        for item in ast.parse(path.read_text()).body:
            if isinstance(item, ast.FunctionDef):
                yield (
                    path.name,
                    item.name,
                    item,
                    getattr(_native, item.name),
                    False,
                    False,
                )
            elif isinstance(item, ast.ClassDef):
                cls = getattr(_native, item.name)
                for method in item.body:
                    if not isinstance(method, ast.FunctionDef):
                        continue
                    decorators = {
                        decorator.id
                        for decorator in method.decorator_list
                        if isinstance(decorator, ast.Name)
                    }
                    if "property" in decorators:
                        getattr(cls, method.name)
                        continue
                    if method.name == "__init__":
                        yield path.name, item.name, method, cls, True, False
                    else:
                        yield (
                            path.name,
                            f"{item.name}.{method.name}",
                            method,
                            getattr(cls, method.name),
                            "classmethod" in decorators,
                            False,
                        )


class Signatures(unittest.TestCase):
    # [spec:pgorm:req:python.typing/test]
    def test_installed_native_signatures_match_shipped_stubs(self):
        root = Path(pgorm.__file__).resolve().parent
        self.assertTrue((root / "py.typed").is_file())
        checked = 0
        for file, name, stub, native, stub_receiver, native_receiver in native_calls(
            root
        ):
            with self.subTest(file=file, name=name):
                parameters = stub_parameters(stub, receiver=stub_receiver)
                if any(parameter[0] == "_native_only" for parameter in parameters):
                    with self.assertRaises(TypeError):
                        native()
                    with self.assertRaises(TypeError):
                        native(object())
                else:
                    self.assertEqual(
                        parameters, runtime_parameters(native, receiver=native_receiver)
                    )
            checked += 1
        self.assertGreater(checked, 200)

    def test_python_wrapper_signatures_match_shipped_stubs(self):
        root = Path(pgorm.__file__).resolve().parent / "_registered"
        for path in sorted(root.glob("*.pyi")):
            module = importlib.import_module("pgorm._registered." + path.stem)
            for cls in ast.parse(path.read_text()).body:
                if not isinstance(cls, ast.ClassDef):
                    continue
                native = getattr(module, cls.name)
                for method in cls.body:
                    if not isinstance(method, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        continue
                    value = getattr(native, method.name)
                    if any(
                        isinstance(decorator, ast.Name) and decorator.id == "property"
                        for decorator in method.decorator_list
                    ):
                        continue
                    with self.subTest(
                        file=path.name, name=cls.name + "." + method.name
                    ):
                        self.assertEqual(
                            stub_parameters(method, receiver=False),
                            runtime_parameters(value, receiver=False),
                        )

    def test_public_exports_exist(self):
        for name in ("pgorm", "pgorm.pipeline", "pgorm.schema", "pgorm.codegen"):
            module = importlib.import_module(name)
            for export in module.__all__:
                with self.subTest(module=name, export=export):
                    self.assertTrue(hasattr(module, export))


if __name__ == "__main__":
    unittest.main()
