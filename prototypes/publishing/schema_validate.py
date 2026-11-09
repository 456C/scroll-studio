from __future__ import annotations

import re


def validate(schema, instance, root=None):
    if root is None:
        root = schema
    if not isinstance(schema, dict):
        return ["schema is not an object"]
    if "$ref" in schema:
        ref = schema["$ref"]
        prefix = "#/$defs/"
        if not isinstance(ref, str) or not ref.startswith(prefix):
            return [f"unsupported ref {ref}"]
        target = root.get("$defs", {}).get(ref[len(prefix) :])
        if target is None:
            return [f"missing ref {ref}"]
        return validate(target, instance, root)
    errors = []
    if "oneOf" in schema:
        matches = [branch for branch in schema["oneOf"] if not validate(branch, instance, root)]
        if len(matches) != 1:
            errors.append(f"oneOf matched {len(matches)}")
        return errors
    if "anyOf" in schema:
        if not any(not validate(branch, instance, root) for branch in schema["anyOf"]):
            errors.append("anyOf matched none")
    if "allOf" in schema:
        for branch in schema["allOf"]:
            errors.extend(validate(branch, instance, root))
    if "const" in schema and instance != schema["const"]:
        errors.append("const")
    if "enum" in schema and instance not in schema["enum"]:
        errors.append("enum")
    if "type" in schema:
        errors.extend(_type_errors(schema["type"], instance))
    if isinstance(instance, str):
        if "minLength" in schema and len(instance) < schema["minLength"]:
            errors.append("minLength")
        if "maxLength" in schema and len(instance) > schema["maxLength"]:
            errors.append("maxLength")
        if "pattern" in schema and re.fullmatch(schema["pattern"], instance) is None:
            errors.append("pattern")
    if type(instance) is int and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            errors.append("minimum")
        if "maximum" in schema and instance > schema["maximum"]:
            errors.append("maximum")
    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            errors.append("minItems")
        if "maxItems" in schema and len(instance) > schema["maxItems"]:
            errors.append("maxItems")
        if "items" in schema:
            for item in instance:
                errors.extend(validate(schema["items"], item, root))
    if isinstance(instance, dict):
        required = schema.get("required", [])
        for key in required:
            if key not in instance:
                errors.append(f"required {key}")
        properties = schema.get("properties", {})
        additional = schema.get("additionalProperties", True)
        for key, value in instance.items():
            if key in properties:
                errors.extend(validate(properties[key], value, root))
            elif additional is False:
                errors.append(f"additional {key}")
    return errors


def _type_errors(expected, instance):
    names = expected if isinstance(expected, list) else [expected]
    if any(_matches_type(name, instance) for name in names):
        return []
    return ["type"]


def _matches_type(name, instance):
    if name == "object":
        return isinstance(instance, dict)
    if name == "array":
        return isinstance(instance, list)
    if name == "string":
        return isinstance(instance, str)
    if name == "boolean":
        return isinstance(instance, bool)
    if name == "integer":
        return type(instance) is int
    if name == "null":
        return instance is None
    return False
