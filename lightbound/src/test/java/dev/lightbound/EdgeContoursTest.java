package dev.lightbound;

import java.util.List;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;
import dev.lightbound.VoidEdgeScanner.Edge;

class EdgeContoursTest {
    @Test void exposedStepEndsJoinWithoutExtendingUnrelatedEdges() {
        var top = new Edge(0,2,0,1,2,0);
        var bottom = new Edge(0,0,0,0,0,1);
        var joined = EdgeContours.joinSteps(List.of(top,bottom), seam -> true);
        assertEquals(3,joined.size());
        assertTrue(joined.contains(new Edge(0,0,0,0,2,0)));
        assertEquals(2, EdgeContours.joinSteps(List.of(top, bottom), seam -> false).size());
        assertEquals(2,EdgeContours.joinSteps(List.of(top,new Edge(4,0,0,4,0,1)), seam -> true).size());
    }
    @Test void roundedLoopStaysClosedAndKeepsItsHeight() {
        var square=List.of(new Edge(0,1,0,1,1,0),new Edge(1,1,0,1,1,1),
                new Edge(1,1,1,0,1,1),new Edge(0,1,1,0,1,0));
        var rounded=EdgeContours.round(square);
        assertEquals(20,rounded.size());
        for(var edge:rounded) {
            assertEquals(1,edge.y1()); assertEquals(1,edge.y2());
            long connections=rounded.stream().filter(other->other!=edge &&
                    (distance(edge.x1(),edge.z1(),other.x1(),other.z1())<1e-8
                    || distance(edge.x1(),edge.z1(),other.x2(),other.z2())<1e-8)).count();
            assertEquals(1,connections,"Every rounded start must connect exactly once");
        }
    }
    private static double distance(double x,double z,double a,double b) { return Math.hypot(x-a,z-b); }
}
