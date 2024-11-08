import { type NextRequest, NextResponse } from "next/server";

// The target URL you want to proxy requests to
const TARGET_URL = "https://api.wandb.ai/graphql";

// Configure CORS headers
const corsHeaders = {
  "Access-Control-Allow-Origin": "*", // Configure this according to your needs
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

// Handle OPTIONS requests for CORS preflight
export async function OPTIONS() {
  return NextResponse.json({}, { headers: corsHeaders });
}

// Handle POST requests
export async function POST(request: NextRequest) {
  try {
    // Get the request body
    const body = await request.json();

    // Get original request headers
    const headers = new Headers();
    request.headers.forEach((value, key) => {
      // Copy relevant headers
      if (
        key.toLowerCase() !== "host" &&
        key.toLowerCase() !== "content-length" &&
        key.toLowerCase() !== "connection"
      ) {
        headers.append(key, value);
      }
    });

    // Forward the request to the target URL
    const response = await fetch(TARGET_URL, {
      method: "POST",
      headers: headers,
      body: JSON.stringify(body),
    });

    // Get the response data
    const data = await response.json();

    // Create the response with CORS headers
    return NextResponse.json(data, {
      status: response.status,
      headers: {
        ...corsHeaders,
        "Content-Type": "application/json",
      },
    });
  } catch (error) {
    console.error("Proxy error:", error);
    return NextResponse.json(
      { error: "Internal Server Error" },
      {
        status: 500,
        headers: {
          ...corsHeaders,
          "Content-Type": "application/json",
        },
      }
    );
  }
}
