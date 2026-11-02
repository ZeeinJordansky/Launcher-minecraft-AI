package minecraft

import (
	"reflect"
	"testing"

	"minty-engine/internal/settings"
)

func TestJVMArgumentsHonorWeakPCMode(t *testing.T) {
	configuration := settings.Defaults()
	configuration.MaxMemoryMB = 4096

	arguments := jvmArguments(configuration, "/tmp/natives")
	if !reflect.DeepEqual(arguments, []string{
		"-Xms1G", "-Xmx4096M", "-Djava.library.path=/tmp/natives",
	}) {
		t.Fatalf("default JVM arguments = %v", arguments)
	}

	configuration.WeakPCMode = true
	arguments = jvmArguments(configuration, "/tmp/natives")
	if !reflect.DeepEqual(arguments, []string{
		"-Xms1G", "-Xmx4096M", "-XX:+UseG1GC", "-XX:MaxGCPauseMillis=100",
		"-Djava.library.path=/tmp/natives",
	}) {
		t.Fatalf("weak-PC JVM arguments = %v", arguments)
	}
}

func TestLaunchRejectsUnmanagedModLoaders(t *testing.T) {
	configuration := settings.Defaults()
	if err := validateLaunchLoader(configuration); err != nil {
		t.Fatalf("Vanilla should be supported: %v", err)
	}

	configuration.ModLoader = "Fabric"
	if err := validateLaunchLoader(configuration); err == nil {
		t.Fatal("launch accepted a mod loader that the engine does not install")
	}
}
