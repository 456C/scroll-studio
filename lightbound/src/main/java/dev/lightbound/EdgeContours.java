package dev.lightbound;

import java.util.*;
import java.util.function.Predicate;
import dev.lightbound.VoidEdgeScanner.Edge;

/** Bounded contour joins. Only open ends at the same terrain corner can join vertically. */
public final class EdgeContours {
    private EdgeContours() {}
    private record Point(double x, double y, double z) {}
    private record Column(double x, double z) {}
    private static Point a(Edge e) { return new Point(e.x1(), e.y1(), e.z1()); }
    private static Point b(Edge e) { return new Point(e.x2(), e.y2(), e.z2()); }
    private static Edge edge(Point a, Point b) { return new Edge(a.x,a.y,a.z,b.x,b.y,b.z); }
    private static Point mix(Point a, Point b, double t) {
        return new Point(a.x+(b.x-a.x)*t,a.y+(b.y-a.y)*t,a.z+(b.z-a.z)*t);
    }
    public static List<Edge> joinSteps(List<Edge> input, Predicate<Edge> bordersVoid) {
        Map<Point,Integer> degree = new HashMap<>();
        for (Edge e:input) { degree.merge(a(e),1,Integer::sum); degree.merge(b(e),1,Integer::sum); }
        Map<Column,List<Point>> columns = new HashMap<>();
        degree.forEach((p,n)-> { if(n==1) columns.computeIfAbsent(new Column(p.x,p.z),k->new ArrayList<>()).add(p); });
        List<Edge> result = new ArrayList<>(input);
        for(var points:columns.values()) {
            points.sort(Comparator.comparingDouble(Point::y));
            if (points.size() == 2 && points.get(1).y - points.get(0).y <= 3
                    && result.size() < VoidEdgeScanner.MAX_SEGMENTS) {
                Edge seam = edge(points.get(0), points.get(1));
                if (bordersVoid.test(seam)) result.add(seam);
            }
        }
        return List.copyOf(result);
    }
    public static List<Edge> round(List<Edge> input) {
        Map<Point,List<Point>> neighbors = new HashMap<>();
        for(Edge e:input) {
            neighbors.computeIfAbsent(a(e),k->new ArrayList<>()).add(b(e));
            neighbors.computeIfAbsent(b(e),k->new ArrayList<>()).add(a(e));
        }
        Set<Point> corners = new HashSet<>();
        neighbors.forEach((p,n)-> {
            if(n.size()==2 && p.y==n.get(0).y && p.y==n.get(1).y
                    && Math.abs((n.get(0).x-p.x)*(n.get(1).z-p.z)-(n.get(0).z-p.z)*(n.get(1).x-p.x))>0.01) corners.add(p);
        });
        List<Edge> result = new ArrayList<>();
        for(Edge e:input) result.add(edge(corners.contains(a(e))?mix(a(e),b(e),0.23):a(e),
                corners.contains(b(e))?mix(b(e),a(e),0.23):b(e)));
        for(Point p:corners) {
            var n=neighbors.get(p); Point from=mix(p,n.get(0),0.23),to=mix(p,n.get(1),0.23),last=from;
            for(int i=1;i<=4;i++) { double t=i/4.0; Point next=mix(mix(from,p,t),mix(p,to,t),t); result.add(edge(last,next)); last=next; }
        }
        return List.copyOf(result);
    }
}
