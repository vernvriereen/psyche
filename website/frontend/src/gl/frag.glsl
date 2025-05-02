precision mediump float;
varying float noiseAmt;
varying float noiseAmt2;
varying vec3 fragNrm;
varying vec3 fragWorldPos;

uniform vec3 lightDir;
uniform vec3 eye;
uniform vec3 ditherColor;

#include dither.glsl

float bell(float _min, float _max, float value) {
  float mid = (_min + _max) / 2.;
  return smoothstep(_min, mid, value) * smoothstep(_max, mid, value);
}

vec3 noiseColor(float n) {
  vec3 col = vec3(.3686, 0., 0.) * smoothstep(.8, 0., n) +
             vec3(.1059, .0627, .7255) * bell(.4, .7, n) +
             vec3(.0627, .7255, .4471) * bell(.5, .8, n) +
             vec3(0., 1., 1.) * smoothstep(.5, 1., n);
  return col;
}

varying highp vec2 vTextureCoord;
varying highp vec3 vLighting;

uniform sampler2D uSampler;

precision highp float;

void main() {

  vec3 nrm = normalize(fragNrm);
  vec3 viewDir = normalize(eye - fragWorldPos);
  vec3 col = noiseColor(noiseAmt);

  float diffuseAmt = max(0.3, dot(nrm, lightDir));
  vec3 diffuseCol = col * diffuseAmt * 0.9;
  gl_FragColor = vec4(diffuseCol, 1.);

  vec3 halfVec = normalize(viewDir + lightDir);
  float specAmt = max(0., dot(nrm, halfVec));
  specAmt = pow(specAmt, 15.);

  vec3 rgbCol = diffuseCol + specAmt;

  gl_FragColor = vec4(ditherColor, dither(rgbCol));
}