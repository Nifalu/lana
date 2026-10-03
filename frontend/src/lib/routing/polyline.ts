/** [longitude, latitude], same order as `LngLat` in the app state. */
type LonLat = [number, number];

/**
 * Decode a Valhalla `polyline6` shape: Google's encoded polyline format,
 * but with 1e6 precision. The encoded pairs are lat/lon; this returns
 * [lon, lat] like everything else in the app.
 *
 * Pure and dependency-free so it can be checked from a plain Node script.
 */
export function decodePolyline6(encoded: string): LonLat[] {
  const points: LonLat[] = [];
  let index = 0;
  let lat = 0;
  let lon = 0;

  const readDelta = (): number => {
    let result = 0;
    let shift = 0;
    let byte: number;
    do {
      if (index >= encoded.length) throw new Error('polyline6: unexpected end of input');
      byte = encoded.charCodeAt(index++) - 63;
      result += (byte & 0x1f) * 2 ** shift;
      shift += 5;
    } while (byte >= 0x20);
    // Zig-zag: the lowest bit is the sign.
    return result % 2 === 1 ? -(result + 1) / 2 : result / 2;
  };

  while (index < encoded.length) {
    lat += readDelta();
    lon += readDelta();
    points.push([lon / 1e6, lat / 1e6]);
  }
  return points;
}
