# server/seed – committed data seeds

## cool-places.geojson

Hand-curated publicly accessible cool places in Basel (museums, libraries,
churches, cafés/plazas without purchase obligation), compiled from the
cantonal heat-prevention page
(https://www.bs.ch/themen/gesundheit/gesundheitsfoerderung/praeventionsangebote/hitze).

**Provenance / geocoding:** the addresses in this file were geocoded **once**
with Nominatim (OpenStreetMap) when the seed was authored. **Addresses are
never geocoded at runtime** – the import loads this file as-is
(`include_str!`, embedded in the server binary).

The file's top-level `metadata` member records the sources and the geocoding
method; per-place geocoding confidence lives in the exploration notes, not in
the file. Coordinates are WGS84 `[lon, lat]`.

To update the seed: edit the GeoJSON, re-run `lana-server import`, and commit.
