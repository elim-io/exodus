"""Rebuild the tile catalog the iOS app ships with: reads tiles.csv, writes ios/Tiles/catalog.json."""
import csv
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    with open(ROOT / "tools" / "tiles.csv") as f:
        tiles = [{"id": row["id"], "colour": row["colour"], "points": int(row["points"])} for row in csv.DictReader(f)]
    with open(ROOT / "ios" / "Tiles" / "catalog.json", "w") as out:
        json.dump({"tiles": tiles}, out, indent=2)


if __name__ == "__main__":
    main()
