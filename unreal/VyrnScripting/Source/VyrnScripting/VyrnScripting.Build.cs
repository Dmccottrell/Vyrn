// Build rules for the VyrnScripting module: links the Vyrn runtime
// library (vyrn.dll / libvyrn.so / libvyrn.dylib) from Source/ThirdParty/Vyrn
// and ships it next to the game.

using System.IO;
using UnrealBuildTool;

public class VyrnScripting : ModuleRules
{
	public VyrnScripting(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

		PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine", "Json" });
		PrivateDependencyModuleNames.AddRange(new string[] { "InputCore", "Projects", "EnhancedInput", "AIModule", "GameplayTasks", "NavigationSystem" });

		string VyrnDir = Path.GetFullPath(Path.Combine(ModuleDirectory, "..", "ThirdParty", "Vyrn"));
		PublicIncludePaths.Add(Path.Combine(VyrnDir, "include"));

		if (Target.Platform == UnrealTargetPlatform.Win64)
		{
			string Dll = Path.Combine(VyrnDir, "Win64", "vyrn.dll");
			// Rust names the import library of vyrn.dll "vyrn.dll.lib".
			PublicAdditionalLibraries.Add(Path.Combine(VyrnDir, "Win64", "vyrn.dll.lib"));
			// Loaded by the module at startup (see VyrnScripting.cpp), so the
			// editor finds it inside the plugin.
			PublicDelayLoadDLLs.Add("vyrn.dll");
			RuntimeDependencies.Add("$(BinaryOutputDir)/vyrn.dll", Dll);
		}
		else if (Target.Platform == UnrealTargetPlatform.Linux)
		{
			string So = Path.Combine(VyrnDir, "Linux", "libvyrn.so");
			PublicAdditionalLibraries.Add(So);
			RuntimeDependencies.Add("$(BinaryOutputDir)/libvyrn.so", So);
		}
		else if (Target.Platform == UnrealTargetPlatform.Mac)
		{
			string Dylib = Path.Combine(VyrnDir, "Mac", "libvyrn.dylib");
			PublicDelayLoadDLLs.Add(Dylib);
			RuntimeDependencies.Add("$(BinaryOutputDir)/libvyrn.dylib", Dylib);
		}
	}
}
