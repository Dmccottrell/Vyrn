// UVyrnScriptComponent: runs a Vyrn script as part of an actor.
//
// Add a "Vyrn Script" component to any actor and set Script to a file in
// Content/Scripts. The script's begin_play(), tick(dt) and end_play()
// functions run with the actor, and the script controls the actor through
// the functions in unreal.vyrn. Saving the file reloads it while the game
// runs.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "Dom/JsonValue.h"

#include "VyrnScriptComponent.generated.h"

struct VyrnEngine;

/** A script called event(name, data). Data is JSON text ("" for none). */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FVyrnScriptEvent, const FString&, Name, const FString&, DataJson);

/**
 * A host function written in C++: gets the script's arguments, gives the
 * result as JSON text ("" for none). To fail, call Component->Fail(...).
 */
using FVyrnHostFunction = TFunction<FString(const TArray<TSharedPtr<FJsonValue>>& Args)>;

UCLASS(ClassGroup = (Scripting), meta = (BlueprintSpawnableComponent), DisplayName = "Vyrn Script")
class VYRNSCRIPTING_API UVyrnScriptComponent : public UActorComponent
{
	GENERATED_BODY()

public:
	UVyrnScriptComponent();

	/** The script to run: a path inside Content/Scripts (such as "door.vyrn"), or a full path. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Vyrn")
	FString Script;

	/** Reload the script whenever its file is saved, while the game runs. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Vyrn")
	bool bHotReload = true;

	/**
	 * Values the script reads with get_var(name). Write numbers as numbers
	 * (180), true/false, or plain text. Scripts can change them with set_var.
	 */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Vyrn")
	TMap<FString, FString> Variables;

	/** Fires when the script calls event(name, data). */
	UPROPERTY(BlueprintAssignable, Category = "Vyrn")
	FVyrnScriptEvent OnScriptEvent;

	/**
	 * Calls one of the script's functions. ArgsJson is a JSON list of the
	 * arguments, such as [10, "red"] (or empty for none). Gives the result as
	 * JSON text; on failure Success is false and Get Last Error says why.
	 */
	UFUNCTION(BlueprintCallable, Category = "Vyrn")
	FString CallFunction(const FString& Name, const FString& ArgsJson, bool& bSuccess);

	/** Whether the script has a function with this name. */
	UFUNCTION(BlueprintPure, Category = "Vyrn")
	bool HasFunction(const FString& Name) const;

	/** Loads the script again from its file. Gives false if it has an error. */
	UFUNCTION(BlueprintCallable, Category = "Vyrn")
	bool ReloadScript();

	/** Why the last load or call failed. */
	UFUNCTION(BlueprintPure, Category = "Vyrn")
	FString GetLastError() const { return LastError; }

	/** The full path of the script file. */
	UFUNCTION(BlueprintPure, Category = "Vyrn")
	FString GetScriptPath() const;

	/**
	 * Adds a host function scripts can call with host.call(Name, ...),
	 * replacing a built-in one of the same name. Call it before BeginPlay
	 * (in the owning actor's constructor or PostInitializeComponents), or
	 * any time after: it applies immediately.
	 */
	void RegisterHostFunction(const FString& Name, FVyrnHostFunction Function);

	/** Inside a host function: makes the script's host.call fail with this message. */
	void Fail(const FString& Message);

	/** An actor by the name scripts know it by (as given by find, spawn, player...). */
	AActor* FindActorByName(const FString& Name);

	/** The name scripts will know an actor by. */
	FString NameForActor(AActor* Actor);

protected:
	virtual void BeginPlay() override;
	virtual void EndPlay(const EEndPlayReason::Type EndPlayReason) override;
	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

private:
	struct FBinding
	{
		UVyrnScriptComponent* Owner = nullptr;
		FString Name;
		FVyrnHostFunction Function;
	};

	static const char* HostThunk(void* User, const char* ArgsJson);
	static void PrintThunk(void* User, const char* Line);

	void RegisterBuiltins();
	void Bind(FBinding& Binding);
	bool Load();
	/** Calls a script function; unset on failure (already logged). */
	TOptional<FString> Call(const FString& Name, const FString& ArgsJson);
	void ReportError(const FString& What);
	void FlushEvents();

	VyrnEngine* Engine = nullptr;
	TMap<FString, TUniquePtr<FBinding>> Bindings;
	TMap<FString, TWeakObjectPtr<AActor>> KnownActors;
	/** Holds a host function's answer until Vyrn has read it. */
	TArray<ANSICHAR> Answer;
	FString LastError;
	FString HostError;
	bool bHostFailed = false;
	/** Events from the script, sent to Blueprints once it has returned. */
	TArray<TPair<FString, FString>> PendingEvents;
	int32 CallDepth = 0;
	FDateTime LoadedStamp;
	float ReloadTimer = 0.f;
	/** Set after an error in tick, to stop repeating it every frame until the file changes. */
	bool bPaused = false;
};
