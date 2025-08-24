import xml.etree.ElementTree as ET
import pprint
from os import walk
import json


def parse_class(file: str):
    tree = ET.parse(file)
    root = tree.getroot()
    name = root.attrib["name"]
    inherits = root.attrib.get("inherits")
    methods = []
    for method in root.findall("./methods/method"):
        method_name = method.attrib["name"]
        return_type = method.find("./return").attrib["type"]
        parameters = []
        for param in method.findall("./param"):
            parameters.append(
                {"name": param.attrib["name"], "type": param.attrib["type"]}
            )
        methods.append(
            {"name": method_name, "return_type": return_type, "parameters": parameters}
        )

    properties = []
    for prop in root.findall("./members/member"):
        prop_name = prop.attrib["name"]
        prop_type = prop.attrib["type"]
        properties.append({"name": prop_name, "type": prop_type})

    output = {
        "name": name,
        "parent": inherits,
        "methods": methods,
        "properties": properties,
    }
    return output


def main():
    cls = parse_class("./classes/Node.xml")
    files = []
    for _, _, filenames in walk("./classes/"):
        files.extend(filenames)
        break
    output = {}
    for file in files:
        cls = parse_class(f"./classes/{file}")
        output[cls["name"]] = cls

    with open("type_info.json", "w+") as f:
        json.dump(output, f, indent=2, sort_keys=True)


if __name__ == "__main__":
    main()
