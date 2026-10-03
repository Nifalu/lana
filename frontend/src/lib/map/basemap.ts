import type {
  ExpressionSpecification,
  FilterSpecification,
  LayerSpecification,
  StyleSpecification,
  SymbolLayerSpecification,
} from 'maplibre-gl';

/** The swisstopo vector styles this app can show. */
export type Basemap = 'standard' | 'imagery';

const STYLE_URL: Record<Basemap, string> = {
  standard: 'https://vectortiles.geo.admin.ch/styles/ch.swisstopo.basemap.vt/style.json',
  imagery: 'https://vectortiles.geo.admin.ch/styles/ch.swisstopo.imagerybasemap.vt/style.json',
};

export function styleUrl(basemap: Basemap): string {
  return STYLE_URL[basemap];
}

// ---------------------------------------------------------------------------
// 1. Layers removed entirely
// ---------------------------------------------------------------------------

const REMOVED_LAYERS = new Set([
  'poi_motorway', // motorway exits, junctions, rest stops, fuel
  'road_number', // route shields (A1, 8, ...)
  'place_other', // suburbs, neighbourhoods, hamlets, localities
  'poi_lm', // landmark pictograms (Landesmuseum, Fernsehturm, ...)
  'boundary', // national + cantonal border line
  'boundary_band', // national + cantonal border halo
  'housenumber', // house numbers (z17+)
  'bathymetry', // lake depth bands (lighter than our water fill, z9-14)
]);

// ---------------------------------------------------------------------------
// 2. Recolors (string replacement anywhere inside paint expressions)
// ---------------------------------------------------------------------------

/** Rail: swisstopo red -> muted grey, so tracks read as infrastructure. */
const RAIL_RECOLOR: Record<string, string> = {
  'rgb(203, 77, 77)': 'rgb(150, 150, 150)', // lines
  'rgb(183, 57, 57)': 'rgb(120, 120, 120)', // labels
};
const RAIL_LAYERS = new Set([
  'tunnel_public_transport',
  'public_transport',
  'l1_public_transport',
  'l2_public_transport_aerialway',
  'transportation_label',
]);

/**
 * Roads: one white fill and one grey casing for every class, instead of
 * yellow motorways and pink/yellow signed routes. Paths and tracks live
 * in separate dashed layers that are not touched.
 */
const ROAD_RECOLOR: Record<string, string> = {
  // fills
  'rgb(248, 207, 117)': 'rgb(255, 255, 255)', // motorway / trunk
  'rgb(250, 182, 158)': 'rgb(255, 255, 255)', // route 5 / 10
  'rgb(250, 243, 158)': 'rgb(255, 255, 255)', // route 6 / 7 / 8
  // casings
  'rgb(70, 55, 30)': 'rgb(60, 60, 60)', // motorway / trunk
  'rgb(75, 55, 25)': 'rgb(60, 60, 60)', // route 5 / 10
  'rgb(65, 65, 25)': 'rgb(60, 60, 60)', // route 6 / 7 / 8
};
const ROAD_LAYERS = new Set([
  'road_casing',
  'road_fill',
  'l1_road_casing',
  'l1_fill',
  'l2_road_casing',
  'l2_fill',
  'transportation_dashes',
  'road_small_dashes',
]);

/** Vegetation: darker greens so parks, meadows and forests stand out. */
const GREEN_RECOLOR: Record<string, string> = {
  'rgb(211, 235, 199)': 'rgb(105, 180, 75)', // park / meadow / golf fallback
  'rgb(231, 243, 225)': 'rgb(140, 200, 115)', // pitch / grass runway
  'rgb(62, 153, 10)': 'rgb(30, 115, 0)', // forest (z10)
  'rgb(62, 168, 0)': 'rgb(30, 125, 0)', // forest (z12+)
  'rgb(138, 198, 108)': 'rgb(80, 150, 55)', // landcover casing
};
const GREEN_LAYERS = new Set(['landcover', 'landcover_casing', 'landuse', 'aeroway_polygon_fill']);

/**
 * swisstopo fades vegetation to 15-35 % opacity at street zoom, which
 * caps how green it can look whatever the color. This replaces the
 * `landcover` opacity curve with a stronger one. Ice keeps its own ramp.
 */
const LANDCOVER_OPACITY: ExpressionSpecification = [
  'interpolate',
  ['exponential', 1.5],
  ['zoom'],
  5, 0,
  6, ['match', ['get', 'class'], 'ice', 0.3, 0.15],
  10, ['match', ['get', 'class'], 'ice', 0.2, 0.3],
  14, [
    'match',
    ['get', 'class'],
    'ice', 0,
    [
      'match',
      ['get', 'subclass'],
      ['forest', 'scrub', 'woody_plant', 'loose_forest'], 0.3,
      ['park', 'golf_course'], 0.6,
      0.5,
    ],
  ],
];

/**
 * Buildings: light grey instead of mid grey. The overlay, green spaces
 * and water are the focus, so individual buildings should recede.
 */
const BUILDING_RECOLOR: Record<string, string> = {
  'rgb(170, 172, 174)': 'rgb(218, 219, 221)', // building fill
  'rgb(196, 198, 200)': 'rgb(228, 229, 231)', // roof / cooling tower fill
  'rgb(154, 156, 158)': 'rgb(200, 201, 203)', // building casing
  'rgb(180, 182, 184)': 'rgb(212, 213, 215)', // roof casing
};
const BUILDING_LAYERS = new Set(['building_fill<z14', 'building_fill', 'building_casing']);

/** Water: darker, stronger blues for fills, lines and labels. */
const WATER_RECOLOR: Record<string, string> = {
  'rgb(210, 238, 255)': 'rgb(100, 170, 230)', // lake / ocean fill, label halos
  'rgb(181, 225, 253)': 'rgb(85, 160, 225)', // river / canal / pool fill
  'rgb(77, 164, 218)': 'rgb(30, 110, 190)', // river lines, shorelines, depth contours
  'rgb(47, 134, 188)': 'rgb(20, 85, 160)', // water label text
  'rgba(220, 241, 254, 0.9)': 'rgba(255, 255, 255, 0.85)', // river label halo
};
const WATER_LAYERS = new Set([
  'water',
  'water_line',
  'water_shoreline',
  'waterway_shoreline',
  'waterway_shoreline_changing',
  'contour_line_blue',
  'waterway_line_label',
  'water_name_point_label',
  'contour_line_pt',
]);

/** Labels drawn on top of the darker water need a light halo, not a blue one. */
const WATER_LABEL_HALO: Record<string, string> = {
  'rgb(100, 170, 230)': 'rgba(255, 255, 255, 0.85)',
};
const WATER_LABEL_LAYERS = new Set(['water_name_point_label', 'contour_line_pt']);

// ---------------------------------------------------------------------------
// 3. Zoom thresholds
// ---------------------------------------------------------------------------

/** Layers that should appear later than swisstopo's default. */
const MIN_ZOOM: Record<string, number> = {
  transportation_label: 15, // street names (default 13)
  landcover_pt: 16, // single trees (default 14)
  pattern_landcover_z12: 14, // tree / scrub / orchard patterns (default 12)
};

// ---------------------------------------------------------------------------
// 4. Filter tightening
// ---------------------------------------------------------------------------

/**
 * POI subclasses hidden from `poi_rank1` / `poi_rank2`: every kind of
 * public transport stop plus parking and other car infrastructure.
 * Public buildings (school, kindergarten, college, university, hospital,
 * place_of_worship, monastery, government, ...) are left alone.
 */
const HIDDEN_POI_SUBCLASSES = [
  'bus_stop',
  'tram_stop',
  'subway_stop',
  'funicular_stop',
  'railway_station',
  'aerialway_station',
  'cable_car_station',
  'gondola_station',
  'chair_lift_station',
  'drag_lift_station',
  'ferry_terminal',
  'ferry',
  'car_ferry',
  'car_shuttle',
  'elevator',
  'parking_public',
  'parking_facility',
  'driving_centre',
];

/** `transportation_label` classes that label public transport lines. */
const HIDDEN_LINE_LABEL_CLASSES = [
  'rail',
  'transit',
  'rail_construction',
  'transit_construction',
  'cable_car',
  'gondola',
  'chair_lift',
  'drag_lift',
  'aerialway_closed',
  'aerialway_goods',
  'aerialway_transportation',
  'goods_conveyor',
  'ferry',
  'car_ferry',
];

const NOT_HIDDEN_POI: ExpressionSpecification = [
  '!',
  ['match', ['get', 'subclass'], HIDDEN_POI_SUBCLASSES, true, false],
];

/**
 * Border admin levels to drop: 2 = national, 4 = cantonal.
 * District (6) and municipal (8) borders stay.
 */
const HIDDEN_ADMIN_LEVELS = [2, 4];
const NOT_HIDDEN_ADMIN: ExpressionSpecification = [
  '!',
  ['match', ['get', 'admin_level'], HIDDEN_ADMIN_LEVELS, true, false],
];
const BOUNDARY_LAYERS = new Set(['boundary_dashed_band', 'boundary_label_r', 'boundary_label_l']);

const NOT_HIDDEN_LINE_LABEL: ExpressionSpecification = [
  '!',
  ['match', ['get', 'class'], HIDDEN_LINE_LABEL_CLASSES, true, false],
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function recolor(value: unknown, mapping: Record<string, string>): unknown {
  if (typeof value === 'string') return mapping[value] ?? value;
  if (Array.isArray(value)) return value.map((v) => recolor(v, mapping));
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) => [k, recolor(v, mapping)]),
    );
  }
  return value;
}

function withRecoloredPaint(
  layer: LayerSpecification,
  mapping: Record<string, string>,
): LayerSpecification {
  if (!('paint' in layer) || !layer.paint) return layer;
  return { ...layer, paint: recolor(layer.paint, mapping) } as LayerSpecification;
}

/**
 * Legacy filter syntax (`["in", "class", "a", "b"]`) and expression syntax
 * (`["match", ["get", "class"], ...]`) cannot be mixed inside one `all`.
 * swisstopo uses both, and the same layer id can use a different syntax
 * in different styles, so every tightening is applied in whichever
 * syntax the existing filter already uses.
 */
function isExpressionFilter(filter: unknown): boolean {
  if (!Array.isArray(filter) || filter.length === 0) return true;
  const [op, ...args] = filter as [string, ...unknown[]];
  switch (op) {
    case 'has':
      return args.length >= 1 && args[0] !== '$id' && args[0] !== '$type';
    case 'in':
      return args.length >= 2 && (typeof args[0] !== 'string' || Array.isArray(args[1]));
    case '!in':
    case '!has':
    case 'none':
      return false;
    case '==':
    case '!=':
    case '>':
    case '>=':
    case '<':
    case '<=':
      return args.length !== 2 || Array.isArray(args[0]) || Array.isArray(args[1]);
    case 'any':
    case 'all':
      return args.every((f) => typeof f === 'boolean' || isExpressionFilter(f));
    default:
      return true;
  }
}

/**
 * AND a condition onto an existing filter. `legacy` and `expression`
 * express the same condition in the two syntaxes; the one matching the
 * existing filter is used.
 */
function andFilter(
  existing: FilterSpecification | undefined,
  legacy: unknown[],
  expression: ExpressionSpecification,
): FilterSpecification {
  if (existing === undefined) return expression;
  if (isExpressionFilter(existing)) {
    return ['all', existing as ExpressionSpecification, expression];
  }
  const f = existing as unknown[];
  const clauses = f[0] === 'all' ? f.slice(1) : [f];
  return ['all', ...clauses, legacy] as unknown as FilterSpecification;
}

function tweakLayer(layer: LayerSpecification): LayerSpecification {
  let out = layer;

  if (RAIL_LAYERS.has(out.id)) out = withRecoloredPaint(out, RAIL_RECOLOR);
  if (ROAD_LAYERS.has(out.id)) out = withRecoloredPaint(out, ROAD_RECOLOR);
  if (GREEN_LAYERS.has(out.id)) out = withRecoloredPaint(out, GREEN_RECOLOR);
  if (BUILDING_LAYERS.has(out.id)) out = withRecoloredPaint(out, BUILDING_RECOLOR);
  if (WATER_LAYERS.has(out.id)) out = withRecoloredPaint(out, WATER_RECOLOR);
  if (WATER_LABEL_LAYERS.has(out.id)) out = withRecoloredPaint(out, WATER_LABEL_HALO);

  if (out.id === 'landcover' && 'paint' in out) {
    out = { ...out, paint: { ...out.paint, 'fill-opacity': LANDCOVER_OPACITY } } as LayerSpecification;
  }

  if (out.id in MIN_ZOOM) out = { ...out, minzoom: MIN_ZOOM[out.id] };

  if ((out.id === 'poi_rank1' || out.id === 'poi_rank2') && 'filter' in out) {
    out = {
      ...out,
      filter: andFilter(out.filter, ['!in', 'subclass', ...HIDDEN_POI_SUBCLASSES], NOT_HIDDEN_POI),
    };
  }

  if (BOUNDARY_LAYERS.has(out.id) && 'filter' in out) {
    out = {
      ...out,
      filter: andFilter(out.filter, ['!in', 'admin_level', ...HIDDEN_ADMIN_LEVELS], NOT_HIDDEN_ADMIN),
    };
  }

  if (out.id === 'transportation_label' && 'filter' in out) {
    out = {
      ...out,
      filter: andFilter(
        out.filter,
        ['!in', 'class', ...HIDDEN_LINE_LABEL_CLASSES],
        NOT_HIDDEN_LINE_LABEL,
      ),
    };
  }

  return out;
}

// ---------------------------------------------------------------------------
// Imagery-only rules
// ---------------------------------------------------------------------------

/** Imagery layers removed: red rail/aerialway lines and national/cantonal borders. */
const IMAGERY_REMOVED_LAYERS = new Set([
  'public_transport',
  'tunnel_public_transport',
  'aerialway',
  'boundary',
  'boundary_disputed',
  'boundary_l_label',
  'boundary_r_label',
]);

/** Imagery road layers that draw motorways / trunks in yellow. */
const IMAGERY_HIGHWAY_LAYERS = new Set(['road_fill', 'tunnel_road']);
const HIGHWAY_CLASSES = ['motorway', 'trunk', 'motorway_construction', 'trunk_construction'];
const NOT_HIGHWAY: ExpressionSpecification = [
  '!',
  ['match', ['get', 'class'], HIGHWAY_CLASSES, true, false],
];

/** POI layers copied from the standard style into the imagery style. */
const POI_LAYERS = ['poi_rank1', 'poi_rank2'];

/** Sprite id under which the standard style's icon sheet is attached to imagery. */
const STANDARD_SPRITE_ID = 'standard';

/** Text paint that reads on top of aerial photos. */
const IMAGERY_POI_TEXT_PAINT = {
  'text-color': 'rgba(255, 255, 250, 1)',
  'text-halo-color': 'rgba(48, 48, 48, 1)',
  'text-halo-width': 2,
  'text-halo-blur': 1,
};

/**
 * Prefix every icon *output* of an `icon-image` expression with a sprite
 * id. Inputs (the subclass / class values being matched) are left alone.
 */
function prefixIconOutputs(value: unknown, prefix: string): unknown {
  if (typeof value === 'string') return value ? `${prefix}:${value}` : value;
  if (!Array.isArray(value)) return value;
  const [op] = value as unknown[];
  if (op === 'match') {
    // ["match", input, label, output, label, output, ..., fallback]
    return value.map((v, i) =>
      i >= 3 && (i % 2 === 1 || i === value.length - 1) ? prefixIconOutputs(v, prefix) : v,
    );
  }
  if (op === 'case') {
    // ["case", cond, output, cond, output, ..., fallback]
    return value.map((v, i) =>
      i >= 2 && (i % 2 === 0 || i === value.length - 1) ? prefixIconOutputs(v, prefix) : v,
    );
  }
  return value;
}

function standardSpriteUrl(standard: StyleSpecification): string | undefined {
  const sprite = standard.sprite;
  if (typeof sprite === 'string') return sprite;
  return sprite?.find((s) => s.id === 'default')?.url ?? sprite?.[0]?.url;
}

function applyImageryRules(
  layers: LayerSpecification[],
  raw: StyleSpecification,
  standard: StyleSpecification,
): { layers: LayerSpecification[]; sprite: StyleSpecification['sprite'] } {
  let out = layers
    .filter((l) => !IMAGERY_REMOVED_LAYERS.has(l.id))
    .map((l) =>
      IMAGERY_HIGHWAY_LAYERS.has(l.id) && 'filter' in l
        ? { ...l, filter: andFilter(l.filter, ['!in', 'class', ...HIGHWAY_CLASSES], NOT_HIGHWAY) }
        : l,
    );

  // Replace the imagery POI layers with the standard ones (already tightened
  // by tweakLayer), icons pointed at the attached standard sprite, text
  // restyled for the photo background.
  const poi = standard.layers
    .filter((l) => POI_LAYERS.includes(l.id))
    .map(tweakLayer)
    .map((l) => {
      const sym = l as SymbolLayerSpecification;
      return {
        ...sym,
        layout: {
          ...sym.layout,
          'icon-image': prefixIconOutputs(sym.layout?.['icon-image'], STANDARD_SPRITE_ID),
        },
        paint: { ...sym.paint, ...IMAGERY_POI_TEXT_PAINT },
      } as LayerSpecification;
    });

  const firstPoi = out.findIndex((l) => POI_LAYERS.includes(l.id));
  out = out.filter((l) => !POI_LAYERS.includes(l.id));
  const at = firstPoi === -1 ? out.length : firstPoi;
  out = [...out.slice(0, at), ...poi, ...out.slice(at)];

  const own = raw.sprite;
  const ownEntries = typeof own === 'string' ? [{ id: 'default', url: own }] : (own ?? []);
  const standardUrl = standardSpriteUrl(standard);
  const sprite = standardUrl ? [...ownEntries, { id: STANDARD_SPRITE_ID, url: standardUrl }] : own;

  return { layers: out, sprite };
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/**
 * Build the final style for a basemap from the raw swisstopo documents.
 * `standard` is always the raw standard style, because the imagery variant
 * borrows its POI layers and sprite.
 */
export function buildStyle(
  basemap: Basemap,
  raw: StyleSpecification,
  standard: StyleSpecification,
): StyleSpecification {
  const layers = raw.layers.filter((l) => !REMOVED_LAYERS.has(l.id)).map(tweakLayer);
  if (basemap === 'standard') return { ...raw, layers };
  const { layers: imageryLayers, sprite } = applyImageryRules(layers, raw, standard);
  return { ...raw, layers: imageryLayers, sprite };
}

const rawStyleCache: Partial<Record<Basemap, Promise<StyleSpecification>>> = {};

function fetchRawStyle(basemap: Basemap): Promise<StyleSpecification> {
  rawStyleCache[basemap] ??= fetch(STYLE_URL[basemap]).then((r) => {
    if (!r.ok) throw new Error(`style ${basemap}: HTTP ${r.status}`);
    return r.json() as Promise<StyleSpecification>;
  });
  return rawStyleCache[basemap]!;
}

/** Fetch (cached) and build the style for a basemap, ready for `map.setStyle`. */
export async function loadStyle(basemap: Basemap): Promise<StyleSpecification> {
  const [raw, standard] = await Promise.all([fetchRawStyle(basemap), fetchRawStyle('standard')]);
  return buildStyle(basemap, raw, standard);
}
