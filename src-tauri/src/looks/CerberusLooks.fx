// Cerberus Launcher looks: picture filters for games, applied through ReShade.
// Written for Cerberus (MIT). One strength, 0 to 1, shared by every look.

uniform float Strength < ui_type = "slider"; ui_min = 0.0; ui_max = 1.0; ui_label = "Intensidade"; > = 0.6;
// The game's own line count for the CRT look; 0 keeps a line every 3 pixels.
uniform float Lines < ui_type = "drag"; ui_min = 0.0; ui_max = 1200.0; ui_label = "Linhas"; > = 0.0;

texture BackBufferTex : COLOR;
sampler BackBuffer { Texture = BackBufferTex; };

static const float2 PIXEL = float2(BUFFER_RCP_WIDTH, BUFFER_RCP_HEIGHT);
static const float3 LUMA = float3(0.2126, 0.7152, 0.0722);

void FullScreenVS(in uint id : SV_VertexID, out float4 pos : SV_Position, out float2 uv : TEXCOORD)
{
    uv.x = (id == 2) ? 2.0 : 0.0;
    uv.y = (id == 1) ? 2.0 : 0.0;
    pos = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
}

float3 Source(float2 uv) { return tex2D(BackBuffer, uv).rgb; }

float Vignette(float2 uv, float amount)
{
    float2 d = uv - 0.5;
    return 1.0 - amount * smoothstep(0.25, 0.75, dot(d, d) * 2.0);
}

// Sharper detail: the picture minus a soft copy of itself, added back.
float4 SharpPS(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target
{
    float3 c = Source(uv);
    float3 soft = (Source(uv + float2(PIXEL.x, 0.0)) + Source(uv - float2(PIXEL.x, 0.0))
                 + Source(uv + float2(0.0, PIXEL.y)) + Source(uv - float2(0.0, PIXEL.y))) * 0.25;
    float3 detail = clamp(c - soft, -0.08, 0.08);
    return float4(saturate(c + detail * Strength * 2.5), 1.0);
}

// Livelier colours: more saturation and a little more contrast.
float4 VividPS(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target
{
    float3 c = Source(uv);
    float l = dot(c, LUMA);
    c = lerp(l.xxx, c, 1.0 + 0.7 * Strength);
    c = (c - 0.5) * (1.0 + 0.2 * Strength) + 0.5;
    return float4(saturate(c), 1.0);
}

// An old TV: a slightly curved screen, dark lines between the rows, the RGB
// stripes of the tube and darker corners.
float4 CrtPS(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target
{
    float2 p = uv * 2.0 - 1.0;
    float2 bent = uv + p * dot(p, p) * 0.035 * Strength;
    if (any(bent < 0.0) || any(bent > 1.0))
        return float4(0.0, 0.0, 0.0, 1.0);
    float3 c = Source(bent);
    float row = Lines > 0.0 ? frac(bent.y * Lines) : frac(pos.y / 3.0);
    float lines = 1.0 - 0.45 * Strength * smoothstep(0.55, 1.0, row);
    float column = frac(pos.x / 3.0);
    float3 stripes = column < 0.333 ? float3(1.0, 0.85, 0.85) : (column < 0.666 ? float3(0.85, 1.0, 0.85) : float3(0.85, 0.85, 1.0));
    float3 mask = lerp(1.0.xxx, stripes, 0.6 * Strength);
    c = c * lines * mask * (1.0 + 0.25 * Strength);
    return float4(saturate(c * Vignette(bent, 0.6 * Strength)), 1.0);
}

// Film: warm tones, softer blacks and darker corners.
float4 FilmPS(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target
{
    float3 c = Source(uv);
    float l = dot(c, LUMA);
    float3 warm = lerp(l.xxx, c, 0.8) * float3(1.14, 1.0, 0.78);
    warm = lerp(warm, smoothstep(0.0, 1.0, warm), 0.6);
    warm = warm * 0.92 + 0.05;
    c = lerp(c, warm, Strength);
    return float4(saturate(c * Vignette(uv, 0.9 * Strength)), 1.0);
}

technique CerberusSharp < ui_label = "Cerberus: Nítido"; > { pass { VertexShader = FullScreenVS; PixelShader = SharpPS; } }
technique CerberusVivid < ui_label = "Cerberus: Cores vivas"; > { pass { VertexShader = FullScreenVS; PixelShader = VividPS; } }
technique CerberusCrt < ui_label = "Cerberus: CRT"; > { pass { VertexShader = FullScreenVS; PixelShader = CrtPS; } }
technique CerberusFilm < ui_label = "Cerberus: Filme"; > { pass { VertexShader = FullScreenVS; PixelShader = FilmPS; } }
