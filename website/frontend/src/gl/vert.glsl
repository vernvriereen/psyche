attribute vec3 position;

uniform mat4 mvp;
uniform mat4 model;
uniform mat3 normal;
uniform float time;

varying float noiseAmt;
varying float noiseAmt2;
varying vec3 fragNrm;
varying vec3 fragWorldPos;

uniform vec2 mousePos;
uniform bool mouseIn;

#include noise.glsl

float noise(vec3 x) {
  float n1 = snoise(vec4(x, time)) * .5 + .5;
  float n2 = snoise(vec4(x * 4., time)) * .5 + .5;
  float n = mix(n1, n1 * n2 * n2, .25);

  return n;
}

float displacement(float n) {
  float m = mix(.65, .95, (sin(time) * .5 + .5));
  return mix(m, 1., n);
}

vec3 calc(float phi, float theta) {
  vec3 p = vec3(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
  p = normalize(p);
  return p * displacement(noise(p));
}

void main() {
  float phi = atan(position.y, position.x);
  float theta = acos(position.z);
  float e = .005;

  float n = noise(position);
  float d = displacement(n);

  float rotationAngle = time * 0.1;
  mat3 rotationMatrix =
      mat3(cos(rotationAngle), 0.0, sin(rotationAngle), 0.0, 1.0, 0.0,
           -sin(rotationAngle), 0.0, cos(rotationAngle));

  vec3 P = rotationMatrix * position * d;

  vec3 repulsionDir = vec3(mousePos.x, mousePos.y, P.z);
  float distToMouse = length(P - repulsionDir);
  float pushStrength = 0.03 / max(0.2, distToMouse * 1.);
  P += normalize(P - repulsionDir) * pushStrength * float(mouseIn);

  vec3 T = rotationMatrix * (calc(phi + e, theta) - position * d);
  vec3 B = rotationMatrix * (calc(phi, theta - e) - position * d);

  gl_Position = mvp * vec4(P, 1.);
  noiseAmt = n;
  noiseAmt2 = noise(position * 20.5);
  fragNrm = normal * normalize(cross(T, B));
  fragWorldPos = (model * vec4(P, 1.)).xyz;
}