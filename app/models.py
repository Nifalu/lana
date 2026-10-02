from datetime import datetime as dt

import pytz
from pydantic.dataclasses import dataclass


@dataclass
class LocationIN:
    longitude: float
    latitude: float


@dataclass
class LocationOUT:
    longitude: float
    latitude: float
    location_updated_at: str = dt.now(tz=pytz.timezone("Europe/Zurich")).isoformat()


@dataclass
class Device:
    device_id: str
    location: LocationOUT


@dataclass
class NeighborhoodResponse:
    neighborhoods: list[str]
