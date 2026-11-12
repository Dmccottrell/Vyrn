#include "VyrnScriptComponent.h"

#include "CollisionQueryParams.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "EngineUtils.h"
#include "GameFramework/Actor.h"
#include "GameFramework/Pawn.h"
#include "GameFramework/PlayerController.h"
#include "HAL/FileManager.h"
#include "InputCoreTypes.h"
#include "Kismet/GameplayStatics.h"
#include "Misc/Paths.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "VyrnScripting.h"
#include "vyrn.h"

// ---- JSON helpers ----------------------------------------------------------

namespace
{
using FArgs = TArray<TSharedPtr<FJsonValue>>;

FString JsonText(const FString& Text)
{
	FString Out = TEXT("\"");
	for (TCHAR C : Text)
	{
		switch (C)
		{
		case TEXT('"'): Out += TEXT("\\\""); break;
		case TEXT('\\'): Out += TEXT("\\\\"); break;
		case TEXT('\n'): Out += TEXT("\\n"); break;
		case TEXT('\r'): Out += TEXT("\\r"); break;
		case TEXT('\t'): Out += TEXT("\\t"); break;
		default:
			if (C < 0x20)
			{
				Out += FString::Printf(TEXT("\\u%04x"), static_cast<int32>(C));
			}
			else
			{
				Out.AppendChar(C);
			}
		}
	}
	Out += TEXT("\"");
	return Out;
}

FString JsonNumber(double N)
{
	// SanitizeFloat never gives "nan"/"inf" style text JSON can't read.
	return FMath::IsFinite(N) ? FString::SanitizeFloat(N) : TEXT("null");
}

FString JsonBool(bool B) { return B ? TEXT("true") : TEXT("false"); }

FString JsonVector(const FVector& V)
{
	return FString::Printf(TEXT("{\"x\":%s,\"y\":%s,\"z\":%s}"), *JsonNumber(V.X), *JsonNumber(V.Y), *JsonNumber(V.Z));
}

FString JsonRotator(const FRotator& R)
{
	return FString::Printf(
		TEXT("{\"pitch\":%s,\"yaw\":%s,\"roll\":%s}"), *JsonNumber(R.Pitch), *JsonNumber(R.Yaw), *JsonNumber(R.Roll));
}

/** A JSON value back to JSON text. */
FString ToJson(const TSharedPtr<FJsonValue>& Value)
{
	if (!Value.IsValid() || Value->IsNull())
	{
		return TEXT("null");
	}
	FString Out;
	FArgs One = {Value};
	TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer =
		TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
	FJsonSerializer::Serialize(One, Writer);
	// Written as [value]: take off the brackets.
	return Out.Mid(1, Out.Len() - 2);
}

bool ParseJsonList(const FString& Text, FArgs& Out)
{
	TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
	return FJsonSerializer::Deserialize(Reader, Out);
}

double Num(const FArgs& A, int32 I, double Default = 0.0)
{
	return A.IsValidIndex(I) && A[I].IsValid() && A[I]->Type == EJson::Number ? A[I]->AsNumber() : Default;
}

bool Bool(const FArgs& A, int32 I, bool Default = false)
{
	return A.IsValidIndex(I) && A[I].IsValid() && A[I]->Type == EJson::Boolean ? A[I]->AsBool() : Default;
}

FString Str(const FArgs& A, int32 I, const FString& Default = FString())
{
	if (!A.IsValidIndex(I) || !A[I].IsValid() || A[I]->IsNull())
	{
		return Default;
	}
	return A[I]->Type == EJson::String ? A[I]->AsString() : ToJson(A[I]);
}

FVector Vec(const FArgs& A, int32 First)
{
	return FVector(Num(A, First), Num(A, First + 1), Num(A, First + 2));
}

FColor ColorNamed(const FString& Name)
{
	const FString N = Name.ToLower();
	if (N.StartsWith(TEXT("#"))) return FColor::FromHex(N);
	if (N == TEXT("red")) return FColor::Red;
	if (N == TEXT("green")) return FColor::Green;
	if (N == TEXT("blue")) return FColor::Blue;
	if (N == TEXT("yellow")) return FColor::Yellow;
	if (N == TEXT("cyan")) return FColor::Cyan;
	if (N == TEXT("orange")) return FColor::Orange;
	if (N == TEXT("purple")) return FColor::Purple;
	if (N == TEXT("black")) return FColor::Black;
	return FColor::White;
}

FString Utf8ToString(const char* Text) { return Text ? FString(UTF8_TO_TCHAR(Text)) : FString(); }
} // namespace

// ---- The component ---------------------------------------------------------

UVyrnScriptComponent::UVyrnScriptComponent()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.bStartWithTickEnabled = true;
}

FString UVyrnScriptComponent::GetScriptPath() const
{
	if (Script.IsEmpty() || !FPaths::IsRelative(Script))
	{
		return Script;
	}
	return FPaths::ConvertRelativePathToFull(FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Scripts"), Script));
}

void UVyrnScriptComponent::BeginPlay()
{
	Super::BeginPlay();
	if (!FVyrnScriptingModule::IsRuntimeLoaded())
	{
		ReportError(TEXT("the Vyrn runtime library is not loaded (see the Output Log)"));
		return;
	}
	if (Script.IsEmpty())
	{
		ReportError(TEXT("no Script is set on this component"));
		return;
	}
	Engine = vyrn_new();
	vyrn_set_print(Engine, &UVyrnScriptComponent::PrintThunk, this);
	RegisterBuiltins();
	for (TPair<FString, TUniquePtr<FBinding>>& Pair : Bindings)
	{
		Bind(*Pair.Value);
	}
	if (Load())
	{
		Call(TEXT("begin_play"), FString());
	}
}

void UVyrnScriptComponent::EndPlay(const EEndPlayReason::Type EndPlayReason)
{
	if (Engine)
	{
		Call(TEXT("end_play"), FString());
		vyrn_free(Engine);
		Engine = nullptr;
	}
	PendingEvents.Empty();
	KnownActors.Empty();
	Super::EndPlay(EndPlayReason);
}

void UVyrnScriptComponent::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);
	if (!Engine)
	{
		return;
	}
	if (bHotReload)
	{
		ReloadTimer -= DeltaTime;
		if (ReloadTimer <= 0.f)
		{
			ReloadTimer = 0.5f;
			const FDateTime Stamp = IFileManager::Get().GetTimeStamp(*GetScriptPath());
			if (Stamp != FDateTime::MinValue() && Stamp != LoadedStamp)
			{
				UE_LOG(LogVyrn, Log, TEXT("%s: reloading %s"), *GetOwner()->GetName(), *Script);
				if (Load())
				{
					// A reloaded script starts fresh: reloaded() lets it pick
					// up where it was; without one, begin_play() runs again.
					Call(vyrn_has_function(Engine, "reloaded") ? TEXT("reloaded") : TEXT("begin_play"), FString());
				}
			}
		}
	}
	if (!bPaused)
	{
		TOptional<FString> Result = Call(TEXT("tick"), FString::Printf(TEXT("[%s]"), *JsonNumber(DeltaTime)));
		if (!Result.IsSet() && vyrn_has_function(Engine, "tick"))
		{
			bPaused = true;
			UE_LOG(LogVyrn, Warning, TEXT("%s: tick() is paused until %s is fixed and saved"),
				*GetOwner()->GetName(), *Script);
		}
	}
}

bool UVyrnScriptComponent::Load()
{
	const FString Path = GetScriptPath();
	LoadedStamp = IFileManager::Get().GetTimeStamp(*Path);
	if (vyrn_load_file(Engine, TCHAR_TO_UTF8(*Path)) != 0)
	{
		ReportError(Utf8ToString(vyrn_last_error(Engine)));
		return false;
	}
	bPaused = false;
	return true;
}

bool UVyrnScriptComponent::ReloadScript()
{
	return Engine && Load();
}

bool UVyrnScriptComponent::HasFunction(const FString& Name) const
{
	return Engine && vyrn_has_function(Engine, TCHAR_TO_UTF8(*Name)) != 0;
}

FString UVyrnScriptComponent::CallFunction(const FString& Name, const FString& ArgsJson, bool& bSuccess)
{
	TOptional<FString> Result = Call(Name, ArgsJson);
	bSuccess = Result.IsSet();
	return Result.Get(FString());
}

TOptional<FString> UVyrnScriptComponent::Call(const FString& Name, const FString& ArgsJson)
{
	if (!Engine)
	{
		LastError = TEXT("the script is not running");
		return {};
	}
	const FTCHARToUTF8 NameUtf8(*Name);
	if (!vyrn_has_function(Engine, reinterpret_cast<const char*>(NameUtf8.Get())))
	{
		// begin_play, tick and friends are all optional.
		LastError = FString::Printf(TEXT("the script has no function `%s`"), *Name);
		return {};
	}
	if (CallDepth > 0)
	{
		LastError = TEXT("a script function cannot be called from inside one of its own host functions");
		ReportError(LastError);
		return {};
	}
	++CallDepth;
	const FTCHARToUTF8 ArgsUtf8(*ArgsJson);
	char* Result = vyrn_call(Engine, reinterpret_cast<const char*>(NameUtf8.Get()),
		ArgsJson.IsEmpty() ? nullptr : reinterpret_cast<const char*>(ArgsUtf8.Get()));
	--CallDepth;
	TOptional<FString> Out;
	if (Result)
	{
		Out = Utf8ToString(Result);
		vyrn_string_free(Result);
	}
	else
	{
		ReportError(Utf8ToString(vyrn_last_error(Engine)));
	}
	FlushEvents();
	return Out;
}

void UVyrnScriptComponent::FlushEvents()
{
	// Sent after the script returns, so Blueprints handling them may call
	// back into the script.
	TArray<TPair<FString, FString>> Events = MoveTemp(PendingEvents);
	for (const TPair<FString, FString>& Event : Events)
	{
		OnScriptEvent.Broadcast(Event.Key, Event.Value);
	}
}

void UVyrnScriptComponent::ReportError(const FString& What)
{
	LastError = What;
	const FString Who = GetOwner() ? GetOwner()->GetName() : TEXT("Vyrn");
	UE_LOG(LogVyrn, Error, TEXT("%s (%s): %s"), *Who, *Script, *What);
	if (GEngine)
	{
		GEngine->AddOnScreenDebugMessage(-1, 8.f, FColor::Red, FString::Printf(TEXT("Vyrn: %s: %s"), *Who, *What));
	}
}

void UVyrnScriptComponent::PrintThunk(void* User, const char* Line)
{
	UVyrnScriptComponent* Self = static_cast<UVyrnScriptComponent*>(User);
	UE_LOG(LogVyrn, Log, TEXT("%s: %s"), Self->GetOwner() ? *Self->GetOwner()->GetName() : TEXT("Vyrn"),
		*Utf8ToString(Line));
}

// ---- Host functions ----------------------------------------------------------

const char* UVyrnScriptComponent::HostThunk(void* User, const char* ArgsJson)
{
	FBinding* Binding = static_cast<FBinding*>(User);
	UVyrnScriptComponent* Self = Binding->Owner;
	FArgs Args;
	if (!ParseJsonList(Utf8ToString(ArgsJson), Args))
	{
		vyrn_host_error(Self->Engine, "the arguments could not be read");
		return nullptr;
	}
	Self->bHostFailed = false;
	const FString Result = Binding->Function(Args);
	if (Self->bHostFailed)
	{
		vyrn_host_error(Self->Engine, TCHAR_TO_UTF8(*Self->HostError));
		return nullptr;
	}
	// Kept in the component until Vyrn has read it.
	const FTCHARToUTF8 Utf8(*Result);
	Self->Answer.Reset();
	Self->Answer.Append(reinterpret_cast<const ANSICHAR*>(Utf8.Get()), Utf8.Length());
	Self->Answer.Add('\0');
	return Self->Answer.GetData();
}

void UVyrnScriptComponent::Fail(const FString& Message)
{
	bHostFailed = true;
	HostError = Message;
}

void UVyrnScriptComponent::Bind(FBinding& Binding)
{
	if (Engine)
	{
		vyrn_register(Engine, TCHAR_TO_UTF8(*Binding.Name), &UVyrnScriptComponent::HostThunk, &Binding);
	}
}

void UVyrnScriptComponent::RegisterHostFunction(const FString& Name, FVyrnHostFunction Function)
{
	TUniquePtr<FBinding> Binding = MakeUnique<FBinding>();
	Binding->Owner = this;
	Binding->Name = Name;
	Binding->Function = MoveTemp(Function);
	Bind(*Binding);
	Bindings.Add(Name, MoveTemp(Binding));
}

FString UVyrnScriptComponent::NameForActor(AActor* Actor)
{
	if (!Actor)
	{
		return FString();
	}
	const FString Name = Actor->GetName();
	KnownActors.Add(Name, Actor);
	return Name;
}

AActor* UVyrnScriptComponent::FindActorByName(const FString& Name)
{
	if (TWeakObjectPtr<AActor>* Known = KnownActors.Find(Name))
	{
		if (Known->IsValid())
		{
			return Known->Get();
		}
	}
	for (TActorIterator<AActor> It(GetWorld()); It; ++It)
	{
		if (It->GetName() == Name)
		{
			KnownActors.Add(Name, *It);
			return *It;
		}
	}
	return nullptr;
}

void UVyrnScriptComponent::RegisterBuiltins()
{
	// Each is skipped if C++ already registered one of the same name, so a
	// game can replace any of them.
	auto Add = [this](const TCHAR* Name, FVyrnHostFunction Function)
	{
		if (!Bindings.Contains(Name))
		{
			RegisterHostFunction(Name, MoveTemp(Function));
		}
	};

	// Messages
	Add(TEXT("log"), [this](const FArgs& A) { UE_LOG(LogVyrn, Log, TEXT("%s: %s"), *GetOwner()->GetName(), *Str(A, 0)); return FString(); });
	Add(TEXT("warn"), [this](const FArgs& A) { UE_LOG(LogVyrn, Warning, TEXT("%s: %s"), *GetOwner()->GetName(), *Str(A, 0)); return FString(); });
	Add(TEXT("error"), [this](const FArgs& A) { UE_LOG(LogVyrn, Error, TEXT("%s: %s"), *GetOwner()->GetName(), *Str(A, 0)); return FString(); });
	Add(TEXT("screen"), [](const FArgs& A)
	{
		if (GEngine)
		{
			GEngine->AddOnScreenDebugMessage(-1, Num(A, 1, 2.0), ColorNamed(Str(A, 2, TEXT("white"))), Str(A, 0));
		}
		return FString();
	});
	Add(TEXT("event"), [this](const FArgs& A)
	{
		const FString Data = A.IsValidIndex(1) && A[1].IsValid() && !A[1]->IsNull() ? ToJson(A[1]) : FString();
		PendingEvents.Emplace(Str(A, 0), Data);
		return FString();
	});

	// The owning actor
	Add(TEXT("owner"), [this](const FArgs&) { return JsonText(NameForActor(GetOwner())); });
	Add(TEXT("get_location"), [this](const FArgs&) { return JsonVector(GetOwner()->GetActorLocation()); });
	Add(TEXT("set_location"), [this](const FArgs& A)
	{
		FHitResult Hit;
		GetOwner()->SetActorLocation(Vec(A, 0), Bool(A, 3), &Hit, ETeleportType::None);
		return JsonBool(!Hit.bBlockingHit);
	});
	Add(TEXT("move"), [this](const FArgs& A)
	{
		FHitResult Hit;
		GetOwner()->AddActorWorldOffset(Vec(A, 0), Bool(A, 3, true), &Hit, ETeleportType::None);
		return JsonBool(!Hit.bBlockingHit);
	});
	Add(TEXT("get_rotation"), [this](const FArgs&) { return JsonRotator(GetOwner()->GetActorRotation()); });
	Add(TEXT("set_rotation"), [this](const FArgs& A)
	{
		GetOwner()->SetActorRotation(FRotator(Num(A, 0), Num(A, 1), Num(A, 2)));
		return FString();
	});
	Add(TEXT("rotate"), [this](const FArgs& A)
	{
		GetOwner()->AddActorWorldRotation(FRotator(Num(A, 0), Num(A, 1), Num(A, 2)));
		return FString();
	});
	Add(TEXT("forward"), [this](const FArgs&) { return JsonVector(GetOwner()->GetActorForwardVector()); });
	Add(TEXT("right"), [this](const FArgs&) { return JsonVector(GetOwner()->GetActorRightVector()); });
	Add(TEXT("get_var"), [this](const FArgs& A)
	{
		const FString* Value = Variables.Find(Str(A, 0));
		if (!Value)
		{
			return FString(TEXT("null"));
		}
		// Numbers, true/false, lists and maps go as themselves; anything
		// else as text.
		FArgs Parsed;
		if (ParseJsonList(FString::Printf(TEXT("[%s]"), **Value), Parsed) && Parsed.Num() == 1)
		{
			return ToJson(Parsed[0]);
		}
		return JsonText(*Value);
	});
	Add(TEXT("set_var"), [this](const FArgs& A)
	{
		Variables.Add(Str(A, 0), Str(A, 1, TEXT("null")));
		return FString();
	});

	// The world
	Add(TEXT("time"), [this](const FArgs&) { return JsonNumber(GetWorld()->GetTimeSeconds()); });
	Add(TEXT("spawn"), [this](const FArgs& A)
	{
		const FString ClassPath = Str(A, 0);
		UClass* Class = LoadClass<AActor>(nullptr, *ClassPath);
		if (!Class)
		{
			Fail(FString::Printf(TEXT("no actor class `%s` (copy its path with right-click > Copy Reference, and add _C for a Blueprint)"), *ClassPath));
			return FString();
		}
		FActorSpawnParameters Params;
		Params.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AdjustIfPossibleButAlwaysSpawn;
		AActor* Spawned = GetWorld()->SpawnActor<AActor>(Class, Vec(A, 1), FRotator(Num(A, 4), Num(A, 5), Num(A, 6)), Params);
		return Spawned ? JsonText(NameForActor(Spawned)) : FString(TEXT("null"));
	});
	Add(TEXT("find"), [this](const FArgs& A)
	{
		TArray<AActor*> Found;
		UGameplayStatics::GetAllActorsWithTag(GetWorld(), FName(*Str(A, 0)), Found);
		TArray<FString> Names;
		for (AActor* Actor : Found)
		{
			Names.Add(JsonText(NameForActor(Actor)));
		}
		return FString::Printf(TEXT("[%s]"), *FString::Join(Names, TEXT(",")));
	});
	Add(TEXT("player"), [this](const FArgs&)
	{
		APawn* Pawn = UGameplayStatics::GetPlayerPawn(GetWorld(), 0);
		return Pawn ? JsonText(NameForActor(Pawn)) : FString(TEXT("null"));
	});
	Add(TEXT("location_of"), [this](const FArgs& A)
	{
		AActor* Actor = FindActorByName(Str(A, 0));
		if (!Actor)
		{
			Fail(FString::Printf(TEXT("there is no actor named `%s`"), *Str(A, 0)));
			return FString();
		}
		return JsonVector(Actor->GetActorLocation());
	});
	Add(TEXT("destroy"), [this](const FArgs& A)
	{
		AActor* Actor = FindActorByName(Str(A, 0));
		const bool bDestroyed = Actor && Actor->Destroy();
		KnownActors.Remove(Str(A, 0));
		return JsonBool(bDestroyed);
	});
	Add(TEXT("trace"), [this](const FArgs& A)
	{
		FHitResult Hit;
		FCollisionQueryParams Params(SCENE_QUERY_STAT(VyrnTrace), true, GetOwner());
		if (!GetWorld()->LineTraceSingleByChannel(Hit, Vec(A, 0), Vec(A, 3), ECC_Visibility, Params))
		{
			return FString(TEXT("null"));
		}
		return FString::Printf(TEXT("{\"actor\":%s,\"point\":%s,\"normal\":%s,\"distance\":%s}"),
			*JsonText(NameForActor(Hit.GetActor())), *JsonVector(Hit.ImpactPoint), *JsonVector(Hit.ImpactNormal),
			*JsonNumber(Hit.Distance));
	});
	Add(TEXT("key_down"), [this](const FArgs& A)
	{
		APlayerController* Controller = UGameplayStatics::GetPlayerController(GetWorld(), 0);
		return JsonBool(Controller && Controller->IsInputKeyDown(FKey(FName(*Str(A, 0)))));
	});
}
