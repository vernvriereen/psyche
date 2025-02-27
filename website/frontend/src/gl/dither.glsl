
float mod2(float a, float b) { return a - (b * floor(a / b)); }

float indexMatrix4x4(int index) {
  if (index == 0)
    return 0.;
  if (index == 1)
    return 8.;
  if (index == 2)
    return 2.;
  if (index == 3)
    return 10.;
  if (index == 4)
    return 12.;
  if (index == 5)
    return 4.;
  if (index == 6)
    return 14.;
  if (index == 7)
    return 6.;
  if (index == 8)
    return 3.;
  if (index == 9)
    return 11.;
  if (index == 10)
    return 1.;
  if (index == 11)
    return 9.;
  if (index == 12)
    return 15.;
  if (index == 13)
    return 7.;
  if (index == 14)
    return 13.;
  if (index == 15)
    return 5.;
}

float indexMatrix8(int index) {
  if (index == 0)
    return 0.;
  if (index == 1)
    return 32.;
  if (index == 2)
    return 8.;
  if (index == 3)
    return 40.;
  if (index == 4)
    return 2.;
  if (index == 5)
    return 34.;
  if (index == 6)
    return 10.;
  if (index == 7)
    return 42.;
  if (index == 8)
    return 48.;
  if (index == 9)
    return 16.;
  if (index == 10)
    return 56.;
  if (index == 11)
    return 24.;
  if (index == 12)
    return 50.;
  if (index == 13)
    return 18.;
  if (index == 14)
    return 58.;
  if (index == 15)
    return 26.;
  if (index == 16)
    return 12.;
  if (index == 17)
    return 44.;
  if (index == 18)
    return 4.;
  if (index == 19)
    return 36.;
  if (index == 20)
    return 14.;
  if (index == 21)
    return 46.;
  if (index == 22)
    return 6.;
  if (index == 23)
    return 38.;
  if (index == 24)
    return 60.;
  if (index == 25)
    return 28.;
  if (index == 26)
    return 52.;
  if (index == 27)
    return 20.;
  if (index == 28)
    return 62.;
  if (index == 29)
    return 30.;
  if (index == 30)
    return 54.;
  if (index == 31)
    return 22.;
  if (index == 32)
    return 3.;
  if (index == 33)
    return 35.;
  if (index == 34)
    return 11.;
  if (index == 35)
    return 43.;
  if (index == 36)
    return 1.;
  if (index == 37)
    return 33.;
  if (index == 38)
    return 9.;
  if (index == 39)
    return 41.;
  if (index == 40)
    return 51.;
  if (index == 41)
    return 19.;
  if (index == 42)
    return 59.;
  if (index == 43)
    return 27.;
  if (index == 44)
    return 49.;
  if (index == 45)
    return 17.;
  if (index == 46)
    return 57.;
  if (index == 47)
    return 25.;
  if (index == 48)
    return 15.;
  if (index == 49)
    return 47.;
  if (index == 50)
    return 7.;
  if (index == 51)
    return 39.;
  if (index == 52)
    return 13.;
  if (index == 53)
    return 45.;
  if (index == 54)
    return 5.;
  if (index == 55)
    return 37.;
  if (index == 56)
    return 63.;
  if (index == 57)
    return 31.;
  if (index == 58)
    return 55.;
  if (index == 59)
    return 23.;
  if (index == 60)
    return 61.;
  if (index == 61)
    return 29.;
  if (index == 62)
    return 53.;
  if (index == 63)
    return 21.;
}

float indexValue() {
  int x = int(mod2(gl_FragCoord.x, 8.));
  int y = int(mod2(gl_FragCoord.y, 8.));
  int index = (x + y * 8);
  return float(indexMatrix8(index)) / 64.0;
}

float dither(vec3 color) {
  highp float x = color.x;
  highp float closestColor = (x < 0.5) ? 0.0 : 1.0;
  highp float secondClosestColor = 1. - closestColor;
  float d = indexValue();
  float distance = abs(x - closestColor);
  return (distance < d) ? closestColor : secondClosestColor;
}
