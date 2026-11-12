// More host functions for scripts: Unreal's reflection (any property, any
// Blueprint-callable function or custom event), other actors, Enhanced
// Input, AI movement, animation montages and multiplayer.

#include "VyrnHostLibrary.h"

#include "AIController.h"
#include "Animation/AnimMontage.h"
#include "Components/ActorComponent.h"
#include "EnhancedInputSubsystems.h"
#include "EnhancedPlayerInput.h"
#include "Engine/LocalPlayer.h"
#include "Engine/World.h"
#include "GameFramework/Character.h"
#include "GameFramework/PlayerController.h"
#include "InputAction.h"
#include "InputMappingContext.h"
#include "Kismet/GameplayStatics.h"
#include "Navigation/PathFollowingComponent.h"
#include "UObject/UnrealType.h"
#include "VyrnJson.h"
#include "VyrnScriptComponent.h"

using namespace VyrnJson;

namespace
{
/** An actor named by a script: "" or "self" is the script's own. */
AActor* ActorNamed(UVyrnScriptComponent* Self, const FString& Name)
{
	if (Name.IsEmpty() || Name == TEXT("self"))
	{
		return Self->GetOwner();
	}
	return Self->FindActorByName(Name);
}

/**
 * The object a property path starts from: "Health" is on the actor,
 * "Mesh.RelativeScale3D" on its component named "Mesh". Gives the object
 * and leaves the last part in PropertyName.
 */
UObject* PropertyOwner(AActor* Actor, const FString& Path, FString& PropertyName)
{
	FString Component;
	if (!Path.Split(TEXT("."), &Component, &PropertyName))
	{
		PropertyName = Path;
		return Actor;
	}
	TArray<UActorComponent*> Components;
	Actor->GetComponents(Components);
	for (UActorComponent* C : Components)
	{
		if (C && (C->GetName() == Component || C->GetFName().ToString() == Component))
		{
			return C;
		}
	}
	return nullptr;
}

/** A property's value as JSON. */
FString PropertyToJson(FProperty* Prop, const void* Ptr, UObject* Owner)
{
	if (FBoolProperty* B = CastField<FBoolProperty>(Prop))
	{
		return JsonBool(B->GetPropertyValue(Ptr));
	}
	if (FNumericProperty* N = CastField<FNumericProperty>(Prop))
	{
		if (N->IsFloatingPoint())
		{
			return JsonNumber(N->GetFloatingPointPropertyValue(Ptr));
		}
		if (N->IsInteger() && N->GetIntPropertyEnum() == nullptr)
		{
			return JsonNumber(static_cast<double>(N->GetSignedIntPropertyValue(Ptr)));
		}
	}
	if (FStrProperty* S = CastField<FStrProperty>(Prop))
	{
		return JsonText(S->GetPropertyValue(Ptr));
	}
	if (FNameProperty* Nm = CastField<FNameProperty>(Prop))
	{
		return JsonText(Nm->GetPropertyValue(Ptr).ToString());
	}
	if (FTextProperty* T = CastField<FTextProperty>(Prop))
	{
		return JsonText(T->GetPropertyValue(Ptr).ToString());
	}
	if (FStructProperty* St = CastField<FStructProperty>(Prop))
	{
		if (St->Struct == TBaseStructure<FVector>::Get())
		{
			return JsonVector(*static_cast<const FVector*>(Ptr));
		}
		if (St->Struct == TBaseStructure<FRotator>::Get())
		{
			return JsonRotator(*static_cast<const FRotator*>(Ptr));
		}
	}
	if (FObjectPropertyBase* O = CastField<FObjectPropertyBase>(Prop))
	{
		UObject* Value = O->GetObjectPropertyValue(Ptr);
		if (!Value)
		{
			return TEXT("null");
		}
		return JsonText(Value->IsA<AActor>() ? Value->GetName() : Value->GetPathName());
	}
	// Anything else in Unreal's own text form, such as an enum's name.
	FString Text;
	Prop->ExportText_Direct(Text, Ptr, Ptr, Owner, PPF_None);
	return JsonText(Text);
}

/** Sets a property from a script's JSON value. Gives why it could not. */
FString PropertyFromJson(UVyrnScriptComponent* Self, FProperty* Prop, void* Ptr, const TSharedPtr<FJsonValue>& V, UObject* Owner)
{
	if (!V.IsValid())
	{
		return TEXT("no value given");
	}
	if (FBoolProperty* B = CastField<FBoolProperty>(Prop))
	{
		B->SetPropertyValue(Ptr, V->AsBool());
		return FString();
	}
	if (FNumericProperty* N = CastField<FNumericProperty>(Prop))
	{
		if (N->IsFloatingPoint())
		{
			N->SetFloatingPointPropertyValue(Ptr, V->AsNumber());
			return FString();
		}
		if (N->IsInteger() && N->GetIntPropertyEnum() == nullptr)
		{
			N->SetIntPropertyValue(Ptr, static_cast<int64>(V->AsNumber()));
			return FString();
		}
	}
	if (FStrProperty* S = CastField<FStrProperty>(Prop))
	{
		S->SetPropertyValue(Ptr, V->AsString());
		return FString();
	}
	if (FNameProperty* Nm = CastField<FNameProperty>(Prop))
	{
		Nm->SetPropertyValue(Ptr, FName(*V->AsString()));
		return FString();
	}
	if (FTextProperty* T = CastField<FTextProperty>(Prop))
	{
		T->SetPropertyValue(Ptr, FText::FromString(V->AsString()));
		return FString();
	}
	if (FStructProperty* St = CastField<FStructProperty>(Prop))
	{
		const TSharedPtr<FJsonObject>* Obj = nullptr;
		if (V->TryGetObject(Obj) && Obj)
		{
			if (St->Struct == TBaseStructure<FVector>::Get())
			{
				*static_cast<FVector*>(Ptr) = FVector((*Obj)->GetNumberField(TEXT("x")), (*Obj)->GetNumberField(TEXT("y")), (*Obj)->GetNumberField(TEXT("z")));
				return FString();
			}
			if (St->Struct == TBaseStructure<FRotator>::Get())
			{
				*static_cast<FRotator*>(Ptr) = FRotator((*Obj)->GetNumberField(TEXT("pitch")), (*Obj)->GetNumberField(TEXT("yaw")), (*Obj)->GetNumberField(TEXT("roll")));
				return FString();
			}
		}
	}
	if (FObjectPropertyBase* O = CastField<FObjectPropertyBase>(Prop))
	{
		if (V->IsNull())
		{
			O->SetObjectPropertyValue(Ptr, nullptr);
			return FString();
		}
		UObject* Value = Self->FindActorByName(V->AsString());
		if (!Value)
		{
			Value = StaticLoadObject(O->PropertyClass, nullptr, *V->AsString());
		}
		if (!Value || !Value->IsA(O->PropertyClass))
		{
			return FString::Printf(TEXT("%s is not a %s"), *V->AsString(), *O->PropertyClass->GetName());
		}
		O->SetObjectPropertyValue(Ptr, Value);
		return FString();
	}
	// Anything else from Unreal's text form.
	const FString Text = V->Type == EJson::String ? V->AsString() : ToJson(V);
	if (!Prop->ImportText_Direct(*Text, Ptr, Owner, PPF_None))
	{
		return FString::Printf(TEXT("cannot read %s as a %s"), *Text, *Prop->GetCPPType());
	}
	return FString();
}

APlayerController* LocalController(UVyrnScriptComponent* Self)
{
	return UGameplayStatics::GetPlayerController(Self->GetWorld(), 0);
}

UEnhancedPlayerInput* EnhancedInput(UVyrnScriptComponent* Self)
{
	APlayerController* PC = LocalController(Self);
	return PC ? Cast<UEnhancedPlayerInput>(PC->PlayerInput) : nullptr;
}

AAIController* AiOf(UVyrnScriptComponent* Self, const FString& Name)
{
	APawn* Pawn = Cast<APawn>(ActorNamed(Self, Name));
	return Pawn ? Cast<AAIController>(Pawn->GetController()) : nullptr;
}
} // namespace

void VyrnRegisterWorldFunctions(UVyrnScriptComponent* Self, const TFunction<void(const TCHAR*, FVyrnHostFunction)>& Add)
{
	// ---- Reflection: any property, any function -------------------------

	// get_property(actor, "Health") or (actor, "Mesh.RelativeScale3D")
	Add(TEXT("get_property"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		if (!Actor)
		{
			Self->Fail(FString::Printf(TEXT("there is no actor named `%s`"), *Str(A, 0)));
			return FString();
		}
		FString Name;
		UObject* Owner = PropertyOwner(Actor, Str(A, 1), Name);
		FProperty* Prop = Owner ? FindFProperty<FProperty>(Owner->GetClass(), FName(*Name)) : nullptr;
		if (!Prop)
		{
			Self->Fail(FString::Printf(TEXT("%s has no property `%s`"), *Actor->GetName(), *Str(A, 1)));
			return FString();
		}
		return PropertyToJson(Prop, Prop->ContainerPtrToValuePtr<void>(Owner), Owner);
	});

	// set_property(actor, "Health", 50)
	Add(TEXT("set_property"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		if (!Actor)
		{
			Self->Fail(FString::Printf(TEXT("there is no actor named `%s`"), *Str(A, 0)));
			return FString();
		}
		FString Name;
		UObject* Owner = PropertyOwner(Actor, Str(A, 1), Name);
		FProperty* Prop = Owner ? FindFProperty<FProperty>(Owner->GetClass(), FName(*Name)) : nullptr;
		if (!Prop)
		{
			Self->Fail(FString::Printf(TEXT("%s has no property `%s`"), *Actor->GetName(), *Str(A, 1)));
			return FString();
		}
		const FString Why = PropertyFromJson(Self, Prop, Prop->ContainerPtrToValuePtr<void>(Owner), A.IsValidIndex(2) ? A[2] : nullptr, Owner);
		if (!Why.IsEmpty())
		{
			Self->Fail(FString::Printf(TEXT("cannot set %s: %s"), *Str(A, 1), *Why));
		}
		return FString();
	});

	// call_function(actor, "OpenDoor", args...): any function Blueprints
	// can call, or a custom event. Gives its return value, or a map of
	// its outputs.
	Add(TEXT("call_function"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		if (!Actor)
		{
			Self->Fail(FString::Printf(TEXT("there is no actor named `%s`"), *Str(A, 0)));
			return FString();
		}
		FString Name;
		UObject* Target = PropertyOwner(Actor, Str(A, 1), Name);
		UFunction* Fn = Target ? Target->FindFunction(FName(*Name)) : nullptr;
		if (!Fn)
		{
			Self->Fail(FString::Printf(TEXT("%s has no function or event `%s`"), *Actor->GetName(), *Str(A, 1)));
			return FString();
		}
		TArray<uint8> Params;
		Params.SetNumZeroed(FMath::Max<int32>(Fn->ParmsSize, 1));
		for (TFieldIterator<FProperty> It(Fn); It && It->HasAnyPropertyFlags(CPF_Parm); ++It)
		{
			It->InitializeValue_InContainer(Params.GetData());
		}
		int32 Next = 2;
		FString Why;
		for (TFieldIterator<FProperty> It(Fn); It && It->HasAnyPropertyFlags(CPF_Parm); ++It)
		{
			const bool bOutput = It->HasAnyPropertyFlags(CPF_ReturnParm)
				|| (It->HasAnyPropertyFlags(CPF_OutParm) && !It->HasAnyPropertyFlags(CPF_ReferenceParm));
			if (bOutput)
			{
				continue;
			}
			if (A.IsValidIndex(Next) && Why.IsEmpty())
			{
				Why = PropertyFromJson(Self, *It, It->ContainerPtrToValuePtr<void>(Params.GetData()), A[Next], Target);
			}
			++Next;
		}
		FString Result = TEXT("null");
		if (Why.IsEmpty())
		{
			Target->ProcessEvent(Fn, Params.GetData());
			TArray<FString> Outputs;
			for (TFieldIterator<FProperty> It(Fn); It && It->HasAnyPropertyFlags(CPF_Parm); ++It)
			{
				const void* Ptr = It->ContainerPtrToValuePtr<void>(Params.GetData());
				if (It->HasAnyPropertyFlags(CPF_ReturnParm))
				{
					Result = PropertyToJson(*It, Ptr, Target);
				}
				else if (It->HasAnyPropertyFlags(CPF_OutParm))
				{
					Outputs.Add(FString::Printf(TEXT("%s:%s"), *JsonText(It->GetName()), *PropertyToJson(*It, Ptr, Target)));
				}
			}
			if (Result == TEXT("null") && Outputs.Num() > 0)
			{
				Result = FString::Printf(TEXT("{%s}"), *FString::Join(Outputs, TEXT(",")));
			}
		}
		for (TFieldIterator<FProperty> It(Fn); It && It->HasAnyPropertyFlags(CPF_Parm); ++It)
		{
			It->DestroyValue_InContainer(Params.GetData());
		}
		if (!Why.IsEmpty())
		{
			Self->Fail(FString::Printf(TEXT("%s: %s"), *Str(A, 1), *Why));
			return FString();
		}
		return Result;
	});

	// ---- Other actors ---------------------------------------------------

	Add(TEXT("set_location_of"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		if (!Actor)
		{
			Self->Fail(FString::Printf(TEXT("there is no actor named `%s`"), *Str(A, 0)));
			return FString();
		}
		FHitResult Hit;
		Actor->SetActorLocation(Vec(A, 1), Bool(A, 4), &Hit, ETeleportType::None);
		return JsonBool(!Hit.bBlockingHit);
	});
	Add(TEXT("rotation_of"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		if (!Actor)
		{
			Self->Fail(FString::Printf(TEXT("there is no actor named `%s`"), *Str(A, 0)));
			return FString();
		}
		return JsonRotator(Actor->GetActorRotation());
	});
	Add(TEXT("set_rotation_of"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		if (!Actor)
		{
			Self->Fail(FString::Printf(TEXT("there is no actor named `%s`"), *Str(A, 0)));
			return FString();
		}
		Actor->SetActorRotation(FRotator(Num(A, 1), Num(A, 2), Num(A, 3)));
		return FString();
	});
	Add(TEXT("class_of"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		return Actor ? JsonText(Actor->GetClass()->GetName()) : FString(TEXT("null"));
	});
	Add(TEXT("tags_of"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		TArray<FString> Tags;
		if (Actor)
		{
			for (const FName& Tag : Actor->Tags)
			{
				Tags.Add(JsonText(Tag.ToString()));
			}
		}
		return FString::Printf(TEXT("[%s]"), *FString::Join(Tags, TEXT(",")));
	});
	// find_class("/Game/Enemies/BP_Enemy.BP_Enemy_C"): every actor of a class.
	Add(TEXT("find_class"), [Self](const FArgs& A)
	{
		UClass* Class = LoadClass<AActor>(nullptr, *Str(A, 0));
		if (!Class)
		{
			Self->Fail(FString::Printf(TEXT("no actor class `%s`"), *Str(A, 0)));
			return FString();
		}
		TArray<AActor*> Found;
		UGameplayStatics::GetAllActorsOfClass(Self->GetWorld(), Class, Found);
		TArray<FString> Names;
		for (AActor* Actor : Found)
		{
			Names.Add(JsonText(Self->NameForActor(Actor)));
		}
		return FString::Printf(TEXT("[%s]"), *FString::Join(Names, TEXT(",")));
	});

	// ---- Enhanced Input ---------------------------------------------------

	// action("/Game/Input/IA_Move.IA_Move"): the action's value now: true or
	// false, a number, or [x, y] / [x, y, z] for 2D and 3D actions.
	Add(TEXT("action"), [Self](const FArgs& A)
	{
		UInputAction* Action = LoadObject<UInputAction>(nullptr, *Str(A, 0));
		if (!Action)
		{
			Self->Fail(FString::Printf(TEXT("no Input Action `%s` (right-click it > Copy Reference)"), *Str(A, 0)));
			return FString();
		}
		UEnhancedPlayerInput* Input = EnhancedInput(Self);
		if (!Input)
		{
			return FString(TEXT("null"));
		}
		const FInputActionValue Value = Input->GetActionValue(Action);
		switch (Action->ValueType)
		{
		case EInputActionValueType::Boolean:
			return JsonBool(Value.Get<bool>());
		case EInputActionValueType::Axis1D:
			return JsonNumber(Value.Get<float>());
		case EInputActionValueType::Axis2D:
		{
			const FVector2D V = Value.Get<FVector2D>();
			return FString::Printf(TEXT("[%s,%s]"), *JsonNumber(V.X), *JsonNumber(V.Y));
		}
		default:
		{
			const FVector V = Value.Get<FVector>();
			return FString::Printf(TEXT("[%s,%s,%s]"), *JsonNumber(V.X), *JsonNumber(V.Y), *JsonNumber(V.Z));
		}
		}
	});
	// add_input_context("/Game/Input/IMC_Default.IMC_Default", priority)
	Add(TEXT("add_input_context"), [Self](const FArgs& A)
	{
		UInputMappingContext* Context = LoadObject<UInputMappingContext>(nullptr, *Str(A, 0));
		APlayerController* PC = LocalController(Self);
		if (!Context)
		{
			Self->Fail(FString::Printf(TEXT("no Input Mapping Context `%s`"), *Str(A, 0)));
			return FString();
		}
		if (PC && PC->GetLocalPlayer())
		{
			if (UEnhancedInputLocalPlayerSubsystem* Sub = ULocalPlayer::GetSubsystem<UEnhancedInputLocalPlayerSubsystem>(PC->GetLocalPlayer()))
			{
				Sub->AddMappingContext(Context, static_cast<int32>(Num(A, 1)));
				return JsonBool(true);
			}
		}
		return JsonBool(false);
	});

	// ---- AI ----------------------------------------------------------------

	// ai_move_to(pawn, x, y, z, accept = 50): walk there on the navigation
	// mesh. Gives false if no path could be started.
	Add(TEXT("ai_move_to"), [Self](const FArgs& A)
	{
		AAIController* AI = AiOf(Self, Str(A, 0));
		if (!AI)
		{
			Self->Fail(FString::Printf(TEXT("`%s` is not a pawn with an AI Controller"), *Str(A, 0)));
			return FString();
		}
		const EPathFollowingRequestResult::Type R = AI->MoveToLocation(Vec(A, 1), static_cast<float>(Num(A, 4, 50.0)));
		return JsonBool(R != EPathFollowingRequestResult::Failed);
	});
	Add(TEXT("ai_stop"), [Self](const FArgs& A)
	{
		if (AAIController* AI = AiOf(Self, Str(A, 0)))
		{
			AI->StopMovement();
		}
		return FString();
	});
	Add(TEXT("ai_moving"), [Self](const FArgs& A)
	{
		AAIController* AI = AiOf(Self, Str(A, 0));
		return JsonBool(AI && AI->GetMoveStatus() == EPathFollowingStatus::Moving);
	});

	// ---- Animation -----------------------------------------------------------

	// play_montage(character, "/Game/Anims/AM_Reload.AM_Reload", rate):
	// gives how long it plays, in seconds.
	Add(TEXT("play_montage"), [Self](const FArgs& A)
	{
		ACharacter* Character = Cast<ACharacter>(ActorNamed(Self, Str(A, 0)));
		UAnimMontage* Montage = LoadObject<UAnimMontage>(nullptr, *Str(A, 1));
		if (!Character || !Montage)
		{
			Self->Fail(Character ? FString::Printf(TEXT("no Anim Montage `%s`"), *Str(A, 1)) : FString::Printf(TEXT("`%s` is not a Character"), *Str(A, 0)));
			return FString();
		}
		return JsonNumber(Character->PlayAnimMontage(Montage, static_cast<float>(Num(A, 2, 1.0))));
	});
	Add(TEXT("stop_montage"), [Self](const FArgs& A)
	{
		if (ACharacter* Character = Cast<ACharacter>(ActorNamed(Self, Str(A, 0))))
		{
			Character->StopAnimMontage();
		}
		return FString();
	});

	// ---- Multiplayer -----------------------------------------------------

	// "standalone", "client", "listen_server" or "dedicated_server".
	Add(TEXT("net_mode"), [Self](const FArgs&)
	{
		switch (Self->GetWorld()->GetNetMode())
		{
		case NM_Client: return JsonText(TEXT("client"));
		case NM_ListenServer: return JsonText(TEXT("listen_server"));
		case NM_DedicatedServer: return JsonText(TEXT("dedicated_server"));
		default: return JsonText(TEXT("standalone"));
		}
	});
	// Whether this copy of the game decides for the actor (the server, or
	// the only copy): game rules run where this is true.
	Add(TEXT("has_authority"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 0));
		return JsonBool(Actor && Actor->HasAuthority());
	});
	Add(TEXT("set_replicates"), [Self](const FArgs& A)
	{
		AActor* Actor = ActorNamed(Self, Str(A, 1));
		if (Actor && Actor->HasAuthority())
		{
			Actor->SetReplicates(Bool(A, 0, true));
			Actor->SetReplicateMovement(Bool(A, 0, true));
		}
		return FString();
	});
}
