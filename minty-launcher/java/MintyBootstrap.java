import java.util.ArrayList;
import java.util.List;

/**
 * Small Java 8-compatible hand-off layer. Rust prepares and verifies the game;
 * this class performs the final JVM process launch and forwards its exit code.
 */
public final class MintyBootstrap {
    private MintyBootstrap() { }

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("Minty bootstrap: expected <java-executable> <minecraft-arguments...>");
            System.exit(64);
            return;
        }

        List<String> command = new ArrayList<String>();
        command.add(args[0]);
        for (int i = 1; i < args.length; i++) {
            command.add(args[i]);
        }

        ProcessBuilder builder = new ProcessBuilder(command);
        builder.inheritIO();
        Process game = builder.start();
        int exitCode = game.waitFor();
        System.exit(exitCode);
    }
}
