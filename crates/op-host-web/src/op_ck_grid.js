// One CanvasKit draw call for the entire dotted grid, preserving clip/transform.
export function paintDots(CK, canvas, points, radius, rgba) {
  if (!points.length || !(radius > 0)) return;
  const paint = new CK.Paint();
  try {
    paint.setAntiAlias(true);
    paint.setColor(CK.Color4f(...rgba));
    paint.setStyle(CK.PaintStyle.Stroke);
    paint.setStrokeCap(CK.StrokeCap.Round);
    paint.setStrokeWidth(radius * 2);
    canvas.drawPoints(CK.PointMode.Points, points, paint);
  } finally { paint.delete(); }
}
