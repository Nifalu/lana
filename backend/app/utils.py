def decode_valhalla_shape(encoded: str) -> list[list[float]]:
    coordinates = []

    index = 0
    lat = 0
    lon = 0

    while index < len(encoded):
        # Latitude
        result = 0
        shift = 0

        while True:
            byte = ord(encoded[index]) - 63
            index += 1

            result |= (byte & 0x1F) << shift
            shift += 5

            if byte < 0x20:
                break

        delta_lat = -(result >> 1) if result & 1 else result >> 1
        lat += delta_lat

        # Longitude
        result = 0
        shift = 0

        while True:
            byte = ord(encoded[index]) - 63
            index += 1

            result |= (byte & 0x1F) << shift
            shift += 5

            if byte < 0x20:
                break

        delta_lon = -(result >> 1) if result & 1 else result >> 1
        lon += delta_lon

        # GeoJSON uses [longitude, latitude]
        coordinates.append(
            [
                lon / 1_000_000,
                lat / 1_000_000,
            ]
        )

    return coordinates
