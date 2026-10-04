from pydantic.dataclasses import dataclass


@dataclass
class LocationIN:
    longitude: float
    latitude: float


@dataclass
class LocationOUT:
    longitude: float
    latitude: float
    location_updated_at: str


@dataclass
class Device:
    device_id: str
    location: LocationOUT


@dataclass
class NeighborhoodResponse:
    neighborhoods: list[str]
